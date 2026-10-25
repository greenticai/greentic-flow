use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, Read, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionKind {
    String,
    Bool,
    Choice,
    Int,
    Float,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    pub kind: QuestionKind,
    pub required: bool,
    pub default: Option<Value>,
    pub choices: Vec<Value>,
    pub show_if: Option<Value>,
    pub writes_to: Option<String>,
}

pub type Answers = HashMap<String, Value>;

#[derive(Debug, Clone)]
pub struct MissingRequired {
    pub missing: Vec<String>,
    pub template: String,
}

impl std::fmt::Display for MissingRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "missing required answers: {}. Provide via --answers/--answers-file. Example:\n{}",
            self.missing.join(", "),
            self.template
        )
    }
}

impl std::error::Error for MissingRequired {}

/// Merge CLI answers with file answers (file answers win).
#[deprecated(
    note = "answers are merged by the CLI layer; this helper is kept for API compatibility"
)]
pub fn merge_answers(cli_answers: Option<Answers>, file_answers: Option<Answers>) -> Answers {
    let mut merged = Answers::new();
    if let Some(cli) = cli_answers {
        merged.extend(cli);
    }
    if let Some(file) = file_answers {
        merged.extend(file);
    }
    merged
}

pub fn validate_required(questions: &[Question], answers: &Answers) -> Result<()> {
    let missing = missing_required(questions, answers);
    if missing.is_empty() {
        return Ok(());
    }
    let template = serde_json::to_string_pretty(&template_for_questions(questions, answers))
        .unwrap_or_else(|_| "{}".to_string());
    Err(MissingRequired { missing, template }.into())
}

pub fn run_interactive(questions: &[Question]) -> Result<Answers> {
    run_interactive_with_seed(questions, Answers::new())
}

pub fn run_interactive_with_seed(questions: &[Question], seed: Answers) -> Result<Answers> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_interactive_with_io(questions, seed, stdin.lock(), stdout.lock())
}

pub fn run_interactive_with_io<R: Read, W: Write>(
    questions: &[Question],
    mut answers: Answers,
    mut reader: R,
    mut writer: W,
) -> Result<Answers> {
    let mut input = String::new();
    for question in questions {
        if !question_visible(question, &answers) {
            continue;
        }
        if answers.contains_key(&question.id) {
            continue;
        }
        let effective_default = question.default.clone();
        loop {
            input.clear();
            write_prompt(&mut writer, question, effective_default.as_ref())?;
            writer.flush().ok();
            let read_any = read_line(&mut reader, &mut input)?;
            let raw = input.trim();
            if raw.is_empty() {
                if let Some(default) = effective_default.clone() {
                    answers.insert(question.id.clone(), default);
                    break;
                }
                if !read_any {
                    return Err(anyhow!(
                        "stdin closed while waiting for answer for '{}'",
                        question.id
                    ));
                }
                if question.required {
                    continue;
                }
                break;
            }
            match parse_answer(raw, question) {
                Ok(value) => {
                    answers.insert(question.id.clone(), value);
                    break;
                }
                Err(err) => {
                    writeln!(writer, "{err}").context("write prompt")?;
                    continue;
                }
            }
        }
    }
    Ok(answers)
}

pub fn extract_questions_from_flow(flow: &Value) -> Result<Vec<Question>> {
    let Some(nodes) = flow.get("nodes").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let mut questions = Vec::new();
    for node in nodes.values() {
        let Some(qnode) = node.get("questions") else {
            continue;
        };
        let fields = qnode
            .get("fields")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("questions node missing fields array"))?;
        for field in fields {
            let id = field
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("questions field missing id"))?;
            let prompt = field
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string();
            let default = field.get("default").cloned();
            let required = field
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(default.is_none());
            let kind = match field.get("type").and_then(Value::as_str) {
                Some("bool") | Some("boolean") => QuestionKind::Bool,
                Some("int") | Some("integer") => QuestionKind::Int,
                Some("float") | Some("number") => QuestionKind::Float,
                Some("choice") | Some("enum") => QuestionKind::Choice,
                _ => QuestionKind::String,
            };
            let choices = field
                .get("options")
                .and_then(Value::as_array)
                .map(|opts| opts.to_vec())
                .unwrap_or_default();
            let show_if = field.get("show_if").cloned();
            questions.push(Question {
                id: id.to_string(),
                prompt,
                kind,
                required,
                default,
                choices,
                show_if,
                writes_to: field
                    .get("writes_to")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
            });
        }
    }
    Ok(questions)
}
fn write_prompt<W: Write>(
    writer: &mut W,
    question: &Question,
    default_override: Option<&Value>,
) -> Result<()> {
    write!(writer, "Question ({}): {}", question.id, question.prompt).context("write prompt")?;
    if let Some(default) = default_override.or(question.default.as_ref()) {
        write!(writer, " [default: {}]", display_value(default)).ok();
    }
    writeln!(writer).ok();
    if question.kind == QuestionKind::Choice && !question.choices.is_empty() {
        for (idx, choice) in question.choices.iter().enumerate() {
            writeln!(writer, "  {}) {}", idx + 1, display_value(choice)).ok();
        }
    }
    Ok(())
}

fn read_line<R: Read>(reader: &mut R, buf: &mut String) -> Result<bool> {
    // The line is edited as a sequence of chars so multi-byte UTF-8 characters are moved and
    // deleted as a unit instead of being split byte by byte.
    let mut chars: Vec<char> = Vec::new();
    let mut cursor = 0usize;
    let mut byte = [0u8; 1];
    let mut read_any = false;
    while reader.read(&mut byte).context("read input")? == 1 {
        read_any = true;
        match byte[0] {
            b'\n' => break,
            b'\r' => continue,
            // Backspace (Ctrl+H) and DEL.
            0x08 | 0x7f => {
                if cursor > 0 {
                    cursor -= 1;
                    chars.remove(cursor);
                }
            }
            // Ctrl+A / Ctrl+E.
            0x01 => cursor = 0,
            0x05 => cursor = chars.len(),
            // ANSI escape sequence (arrow keys/home/end/delete).
            0x1b => consume_escape_sequence(reader, &mut chars, &mut cursor)?,
            b if b.is_ascii_control() => {}
            b => {
                let ch = read_utf8_char(reader, b)?;
                chars.insert(cursor, ch);
                cursor += 1;
            }
        }
    }
    *buf = chars.into_iter().collect();
    Ok(read_any)
}

/// Decode one UTF-8 char whose first byte has already been read.
fn read_utf8_char<R: Read>(reader: &mut R, first: u8) -> Result<char> {
    let extra = match first {
        0x00..=0x7f => 0,
        0xc0..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf7 => 3,
        _ => return Err(anyhow!("parse input as UTF-8: invalid leading byte")),
    };
    let mut encoded = [0u8; 4];
    encoded[0] = first;
    for slot in encoded.iter_mut().skip(1).take(extra) {
        let mut next = [0u8; 1];
        if reader.read(&mut next).context("read input")? != 1 {
            return Err(anyhow!("parse input as UTF-8: truncated character"));
        }
        *slot = next[0];
    }
    let text = std::str::from_utf8(&encoded[..=extra]).context("parse input as UTF-8")?;
    text.chars()
        .next()
        .ok_or_else(|| anyhow!("parse input as UTF-8: empty character"))
}

fn consume_escape_sequence<R: Read>(
    reader: &mut R,
    chars: &mut Vec<char>,
    cursor: &mut usize,
) -> Result<()> {
    let mut next = [0u8; 1];
    if reader.read(&mut next).context("read escape sequence")? != 1 {
        return Ok(());
    }
    if next[0] != b'[' {
        return Ok(());
    }

    let mut seq = Vec::new();
    loop {
        let mut b = [0u8; 1];
        if reader.read(&mut b).context("read escape sequence")? != 1 {
            return Ok(());
        }
        seq.push(b[0]);
        if b[0].is_ascii_alphabetic() || b[0] == b'~' {
            break;
        }
        if seq.len() > 8 {
            return Ok(());
        }
    }

    match seq.as_slice() {
        [b'D'] if *cursor > 0 => {
            *cursor -= 1;
        }
        [b'C'] if *cursor < chars.len() => {
            *cursor += 1;
        }
        [b'H'] | [b'1', b'~'] | [b'7', b'~'] => *cursor = 0,
        [b'F'] | [b'4', b'~'] | [b'8', b'~'] => *cursor = chars.len(),
        [b'3', b'~'] if *cursor < chars.len() => {
            chars.remove(*cursor);
        }
        _ => {}
    }
    Ok(())
}

fn parse_answer(raw: &str, question: &Question) -> Result<Value> {
    match question.kind {
        QuestionKind::String => Ok(Value::String(raw.to_string())),
        QuestionKind::Bool => parse_bool(raw).map(Value::Bool),
        QuestionKind::Int => {
            let parsed = raw.parse::<i64>().map_err(|_| anyhow!("invalid integer"))?;
            Ok(Value::Number(parsed.into()))
        }
        QuestionKind::Float => {
            let parsed = raw.parse::<f64>().map_err(|_| anyhow!("invalid number"))?;
            let number =
                serde_json::Number::from_f64(parsed).ok_or_else(|| anyhow!("invalid number"))?;
            Ok(Value::Number(number))
        }
        QuestionKind::Choice => parse_choice(raw, question),
    }
}

fn parse_bool(raw: &str) -> Result<bool> {
    let lowered = raw.trim().to_lowercase();
    let compact: String = lowered.chars().filter(|c| !c.is_whitespace()).collect();
    match compact.as_str() {
        "yes=true" => Ok(true),
        "no=false" => Ok(false),
        "y" | "yes" | "true" | "1" => Ok(true),
        "n" | "no" | "false" | "0" => Ok(false),
        _ => Err(anyhow!("invalid boolean")),
    }
}

fn parse_choice(raw: &str, question: &Question) -> Result<Value> {
    if let Ok(idx) = raw.parse::<usize>()
        && idx >= 1
        && idx <= question.choices.len()
    {
        return Ok(question.choices[idx - 1].clone());
    }
    for choice in &question.choices {
        if display_value(choice) == raw {
            return Ok(choice.clone());
        }
    }
    Err(anyhow!("invalid choice"))
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn apply_writes_to(
    mut base: Value,
    questions: &[Question],
    answers: &Answers,
) -> Result<Value> {
    for question in questions {
        let Some(path) = question.writes_to.as_deref() else {
            continue;
        };
        let Some(answer) = answers.get(&question.id) else {
            continue;
        };
        let tokens = parse_path_tokens(path)?;
        set_value_at_path(&mut base, &tokens, answer.clone());
    }
    Ok(base)
}

pub fn extract_answers_from_payload(questions: &[Question], payload: &Value) -> Answers {
    let mut answers = Answers::new();
    for question in questions {
        let Some(path) = question.writes_to.as_deref() else {
            continue;
        };
        if let Ok(tokens) = parse_path_tokens(path)
            && let Some(value) = get_value_at_path(payload, &tokens)
        {
            answers.insert(question.id.clone(), value);
        }
    }
    answers
}

/// Upper bound for array indexes in `writes_to` paths; protects against huge allocations.
const MAX_WRITES_TO_INDEX: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathToken {
    Key(String),
    Index(usize),
}

fn parse_path_tokens(path: &str) -> Result<Vec<PathToken>> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !buf.is_empty() {
                    tokens.push(PathToken::Key(std::mem::take(&mut buf)));
                }
            }
            '[' => {
                if !buf.is_empty() {
                    tokens.push(PathToken::Key(std::mem::take(&mut buf)));
                }
                let mut idx_buf = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == ']' {
                        closed = true;
                        break;
                    }
                    idx_buf.push(c);
                }
                if !closed {
                    return Err(anyhow!("unclosed '[' in writes_to path '{path}'"));
                }
                let idx = idx_buf
                    .parse::<usize>()
                    .map_err(|_| anyhow!("invalid index in writes_to path '{path}'"))?;
                if idx > MAX_WRITES_TO_INDEX {
                    return Err(anyhow!(
                        "index {idx} in writes_to path '{path}' exceeds maximum of {MAX_WRITES_TO_INDEX}"
                    ));
                }
                tokens.push(PathToken::Index(idx));
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        tokens.push(PathToken::Key(buf));
    }
    if tokens.is_empty() {
        Err(anyhow!("writes_to path is empty"))
    } else {
        Ok(tokens)
    }
}

fn ensure_array_len(arr: &mut Vec<Value>, index: usize) {
    if arr.len() <= index {
        arr.resize(index + 1, Value::Null);
    }
}

fn set_value_at_path(target: &mut Value, tokens: &[PathToken], value: Value) {
    let mut current = target;
    for (i, token) in tokens.iter().enumerate() {
        let last = i == tokens.len() - 1;
        match token {
            PathToken::Key(key) => {
                if !current.is_object() {
                    *current = Value::Object(serde_json::Map::new());
                }
                let Some(obj) = current.as_object_mut() else {
                    return;
                };
                if last {
                    obj.insert(key.clone(), value);
                    return;
                }
                current = obj.entry(key.clone()).or_insert(Value::Null);
            }
            PathToken::Index(index) => {
                if !current.is_array() {
                    *current = Value::Array(Vec::new());
                }
                let Some(arr) = current.as_array_mut() else {
                    return;
                };
                ensure_array_len(arr, *index);
                if last {
                    arr[*index] = value;
                    return;
                }
                current = &mut arr[*index];
            }
        }
    }
}

fn get_value_at_path(target: &Value, tokens: &[PathToken]) -> Option<Value> {
    let mut current = target;
    for token in tokens {
        match token {
            PathToken::Key(key) => {
                current = current.as_object()?.get(key)?;
            }
            PathToken::Index(index) => {
                current = current.as_array()?.get(*index)?;
            }
        }
    }
    Some(current.clone())
}

fn missing_required(questions: &[Question], answers: &Answers) -> Vec<String> {
    questions
        .iter()
        .filter(|q| q.required && question_visible(q, answers) && !answers.contains_key(&q.id))
        .map(|q| q.id.clone())
        .collect::<Vec<_>>()
}

fn template_for_questions(questions: &[Question], answers: &Answers) -> Value {
    let mut obj = serde_json::Map::new();
    for question in questions {
        if !question_visible(question, answers) {
            continue;
        }
        let value = if let Some(default) = question.default.clone() {
            default
        } else {
            match question.kind {
                QuestionKind::Bool => Value::Bool(false),
                QuestionKind::Int => Value::Number(0.into()),
                QuestionKind::Float => Value::Number(
                    serde_json::Number::from_f64(0.0)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                ),
                QuestionKind::Choice => question
                    .choices
                    .first()
                    .cloned()
                    .unwrap_or_else(|| Value::String(String::new())),
                QuestionKind::String => Value::String(String::new()),
            }
        };
        obj.insert(question.id.clone(), value);
    }
    Value::Object(obj)
}

pub(crate) fn question_visible(question: &Question, answers: &Answers) -> bool {
    let Some(show_if) = &question.show_if else {
        return true;
    };
    match show_if {
        Value::Bool(value) => *value,
        Value::Object(map) => {
            let Some(id) = map.get("id").and_then(Value::as_str) else {
                return true;
            };
            let Some(expected) = map.get("equals") else {
                return true;
            };
            let Some(actual) = answers.get(id) else {
                return false;
            };
            actual == expected
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn interactive_accepts_default_on_empty() {
        let question = Question {
            id: "name".to_string(),
            prompt: "Name?".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: Some(Value::String("Ada".to_string())),
            choices: Vec::new(),
            show_if: None,
            writes_to: None,
        };
        let input = Cursor::new("\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&[question], Answers::new(), input, output).unwrap();
        assert_eq!(answers.get("name"), Some(&Value::String("Ada".to_string())));
    }

    #[test]
    fn choice_accepts_index_or_value() {
        let question = Question {
            id: "color".to_string(),
            prompt: "Color?".to_string(),
            kind: QuestionKind::Choice,
            required: true,
            default: None,
            choices: vec![
                Value::String("red".to_string()),
                Value::String("blue".to_string()),
            ],
            show_if: None,
            writes_to: None,
        };
        let input = Cursor::new("2\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(
            std::slice::from_ref(&question),
            Answers::new(),
            input,
            output,
        )
        .unwrap();
        assert_eq!(
            answers.get("color"),
            Some(&Value::String("blue".to_string()))
        );

        let input = Cursor::new("red\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&[question], Answers::new(), input, output).unwrap();
        assert_eq!(
            answers.get("color"),
            Some(&Value::String("red".to_string()))
        );
    }

    #[test]
    fn missing_required_reports_all_fields() {
        let questions = vec![
            Question {
                id: "a".to_string(),
                prompt: "A?".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
            Question {
                id: "b".to_string(),
                prompt: "B?".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
        ];
        let err = validate_required(&questions, &Answers::new()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("a"));
        assert!(msg.contains("b"));
        assert!(msg.contains("--answers"));
        assert!(msg.contains('{'));
    }

    #[test]
    fn interactive_parses_int_and_bool() {
        let questions = vec![
            Question {
                id: "count".to_string(),
                prompt: "Count?".to_string(),
                kind: QuestionKind::Int,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
            Question {
                id: "flag".to_string(),
                prompt: "Flag?".to_string(),
                kind: QuestionKind::Bool,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
        ];
        let input = Cursor::new("42\ny\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&questions, Answers::new(), input, output).unwrap();
        assert_eq!(answers.get("count"), Some(&Value::Number(42.into())));
        assert_eq!(answers.get("flag"), Some(&Value::Bool(true)));
    }

    #[test]
    fn interactive_accepts_yes_no_equals_true_false() {
        let questions = vec![
            Question {
                id: "enabled".to_string(),
                prompt: "Enabled?".to_string(),
                kind: QuestionKind::Bool,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
            Question {
                id: "disabled".to_string(),
                prompt: "Disabled?".to_string(),
                kind: QuestionKind::Bool,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
        ];
        let input = Cursor::new("YeS = TrUe\nNo = False\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&questions, Answers::new(), input, output).unwrap();
        assert_eq!(answers.get("enabled"), Some(&Value::Bool(true)));
        assert_eq!(answers.get("disabled"), Some(&Value::Bool(false)));
    }

    #[test]
    fn interactive_respects_show_if_equals() {
        let questions = vec![
            Question {
                id: "mode".to_string(),
                prompt: "Mode?".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: Some(Value::String("asset".to_string())),
                choices: Vec::new(),
                show_if: None,
                writes_to: None,
            },
            Question {
                id: "asset_path".to_string(),
                prompt: "Asset?".to_string(),
                kind: QuestionKind::String,
                required: true,
                default: None,
                choices: Vec::new(),
                show_if: Some(json!({ "id": "mode", "equals": "asset" })),
                writes_to: None,
            },
        ];
        let input = Cursor::new("\npath.json\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&questions, Answers::new(), input, output).unwrap();
        assert_eq!(
            answers.get("mode"),
            Some(&Value::String("asset".to_string()))
        );
        assert_eq!(
            answers.get("asset_path"),
            Some(&Value::String("path.json".to_string()))
        );

        let input = Cursor::new("inline\n");
        let output = Vec::new();
        let answers = run_interactive_with_io(&questions, Answers::new(), input, output).unwrap();
        assert_eq!(
            answers.get("mode"),
            Some(&Value::String("inline".to_string()))
        );
        assert!(!answers.contains_key("asset_path"));
    }

    #[test]
    fn validate_required_skips_hidden_questions() {
        let questions = vec![Question {
            id: "hidden".to_string(),
            prompt: "Hidden?".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: Some(Value::Bool(false)),
            writes_to: None,
        }];
        validate_required(&questions, &Answers::new()).unwrap();
    }

    #[test]
    fn writes_to_creates_nested_objects() {
        let questions = vec![Question {
            id: "asset_path".to_string(),
            prompt: "Asset?".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: None,
            writes_to: Some("card_spec.asset_path".to_string()),
        }];
        let mut answers = Answers::new();
        answers.insert(
            "asset_path".to_string(),
            Value::String("path.json".to_string()),
        );
        let output = apply_writes_to(Value::Object(Default::default()), &questions, &answers)
            .expect("apply");
        let card_spec = output.get("card_spec").and_then(Value::as_object).unwrap();
        assert_eq!(
            card_spec.get("asset_path").and_then(Value::as_str),
            Some("path.json")
        );
    }

    #[test]
    fn writes_to_supports_array_indexes() {
        let questions = vec![Question {
            id: "action_id".to_string(),
            prompt: "Action?".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: None,
            writes_to: Some("actions[0].id".to_string()),
        }];
        let mut answers = Answers::new();
        answers.insert(
            "action_id".to_string(),
            Value::String("action-1".to_string()),
        );
        let output = apply_writes_to(Value::Object(Default::default()), &questions, &answers)
            .expect("apply");
        let actions = output.get("actions").and_then(Value::as_array).unwrap();
        let first = actions[0].as_object().unwrap();
        assert_eq!(first.get("id").and_then(Value::as_str), Some("action-1"));
    }

    #[test]
    fn read_line_supports_backspace_and_arrow_edits() {
        let mut input = Cursor::new(b"abc\x1b[D\x1b[D\x7fX\n".to_vec());
        let mut buf = String::new();
        let read_any = read_line(&mut input, &mut buf).expect("read line");
        assert!(read_any);
        assert_eq!(buf, "Xbc");
    }

    #[test]
    fn read_line_supports_home_end_and_delete() {
        let mut input = Cursor::new(b"abcd\x1b[D\x1b[D\x1b[D\x1b[3~X\x1b[H*\x1b[F!\n".to_vec());
        let mut buf = String::new();
        let read_any = read_line(&mut input, &mut buf).expect("read line");
        assert!(read_any);
        assert_eq!(buf, "*aXcd!");
    }

    #[test]
    fn read_line_edits_multibyte_chars_as_units() {
        // "café", Left x2, Backspace removes 'a', then insert X; no byte-level corruption.
        let mut input = Cursor::new("caf\u{e9}\x1b[D\x1b[D\x7fX\n".as_bytes().to_vec());
        let mut buf = String::new();
        read_line(&mut input, &mut buf).expect("read line");
        assert_eq!(buf, "cXf\u{e9}");

        // Backspace directly over a multi-byte char.
        let mut input = Cursor::new("a\u{e9}\x7fb\n".as_bytes().to_vec());
        let mut buf = String::new();
        read_line(&mut input, &mut buf).expect("read line");
        assert_eq!(buf, "ab");

        // Left arrow over a multi-byte char then insert.
        let mut input = Cursor::new("\u{4f60}\u{597d}\x1b[DX\n".as_bytes().to_vec());
        let mut buf = String::new();
        read_line(&mut input, &mut buf).expect("read line");
        assert_eq!(buf, "\u{4f60}X\u{597d}");
    }

    #[test]
    fn writes_to_rejects_huge_index() {
        let q = |path: &str| Question {
            id: "x".to_string(),
            prompt: "x".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: None,
            writes_to: Some(path.to_string()),
        };
        let mut answers = Answers::new();
        answers.insert("x".to_string(), Value::String("v".to_string()));
        let base = || Value::Object(Default::default());
        let err = apply_writes_to(base(), &[q("a[18446744073709551615]")], &answers)
            .expect_err("overflowing index must error");
        assert!(err.to_string().contains("exceeds maximum"));
        assert!(apply_writes_to(base(), &[q("a[999999999999]")], &answers).is_err());
        assert!(apply_writes_to(base(), &[q("a[1024]")], &answers).is_ok());
    }

    #[test]
    fn writes_to_rejects_unclosed_bracket() {
        let questions = vec![Question {
            id: "x".to_string(),
            prompt: "x".to_string(),
            kind: QuestionKind::String,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: None,
            writes_to: Some("a[0".to_string()),
        }];
        let mut answers = Answers::new();
        answers.insert("x".to_string(), Value::String("v".to_string()));
        let err = apply_writes_to(Value::Object(Default::default()), &questions, &answers)
            .expect_err("unclosed bracket must error");
        assert!(err.to_string().contains("unclosed"));
    }

    #[test]
    fn interactive_prints_parse_error_before_reprompt() {
        let question = Question {
            id: "count".to_string(),
            prompt: "Count?".to_string(),
            kind: QuestionKind::Int,
            required: true,
            default: None,
            choices: Vec::new(),
            show_if: None,
            writes_to: None,
        };
        let mut output = Vec::new();
        let answers = run_interactive_with_io(
            &[question],
            Answers::new(),
            Cursor::new("abc\n7\n"),
            &mut output,
        )
        .unwrap();
        assert_eq!(answers.get("count"), Some(&Value::Number(7.into())));
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("invalid integer")
        );
    }
}
