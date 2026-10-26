use crate::error::{FlowError, FlowErrorLocation, Result};
use crate::i18n::{I18nCatalog, resolve_cli_template, resolve_cli_text, resolve_text};
use greentic_types::schemas::component::v0_6_0::{ComponentQaSpec, QuestionKind};
use qa_spec::FormSpec;
use qa_spec::spec::question::{QuestionSpec, QuestionType};
use serde_json::{Map, Number, Value};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};

pub fn warn_unknown_keys(
    answers: &HashMap<String, Value>,
    spec: &ComponentQaSpec,
    catalog: &I18nCatalog,
    locale: &str,
) {
    let mut known = std::collections::BTreeSet::new();
    for question in &spec.questions {
        known.insert(question.id.as_str());
    }
    let mut unknown = Vec::new();
    for key in answers.keys() {
        if !known.contains(key.as_str()) {
            unknown.push(key.clone());
        }
    }
    if !unknown.is_empty() {
        let unknown_csv = unknown.join(", ");
        eprintln!(
            "{}",
            resolve_cli_template(
                catalog,
                locale,
                "cli.qa.warning.unknown_answers_keys",
                "warning: answers include unknown keys: {}",
                &[unknown_csv.as_str()],
            )
        );
    }
}

pub fn run_interactive(
    spec: &ComponentQaSpec,
    catalog: &I18nCatalog,
    locale: &str,
    answers: HashMap<String, Value>,
) -> Result<HashMap<String, Value>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_interactive_with_io(spec, catalog, locale, answers, stdin.lock(), stdout.lock())
}

fn run_interactive_with_io<R: BufRead, W: Write>(
    spec: &ComponentQaSpec,
    catalog: &I18nCatalog,
    locale: &str,
    mut answers: HashMap<String, Value>,
    mut reader: R,
    mut writer: W,
) -> Result<HashMap<String, Value>> {
    let form = component_spec_to_form(spec, catalog, locale);
    let inline_json_ids = inline_json_ids(spec);
    writeln!(writer, "{}", resolve_text(&spec.title, catalog, locale)).map_err(|err| {
        FlowError::Internal {
            message: format!("write prompt: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        }
    })?;
    if let Some(desc) = spec.description.as_ref() {
        writeln!(writer, "{}", resolve_text(desc, catalog, locale)).map_err(|err| {
            FlowError::Internal {
                message: format!("write prompt: {err}"),
                location: FlowErrorLocation::new(None, None, None),
            }
        })?;
    }

    for question in &spec.questions {
        if answers.contains_key(&question.id) {
            continue;
        }
        loop {
            let label = resolve_text(&question.label, catalog, locale);
            if let Some(help) = question.help.as_ref() {
                writeln!(
                    writer,
                    "{} ({})",
                    label,
                    resolve_text(help, catalog, locale)
                )
                .map_err(|err| FlowError::Internal {
                    message: format!("write prompt: {err}"),
                    location: FlowErrorLocation::new(None, None, None),
                })?;
            } else {
                writeln!(writer, "{label}").map_err(|err| FlowError::Internal {
                    message: format!("write prompt: {err}"),
                    location: FlowErrorLocation::new(None, None, None),
                })?;
            }
            let prompt = match &question.kind {
                QuestionKind::Choice { options } => {
                    for (idx, option) in (1usize..).zip(options.iter()) {
                        let option_label = resolve_text(&option.label, catalog, locale);
                        writeln!(writer, "  {idx}. {option_label} ({})", option.value).map_err(
                            |err| FlowError::Internal {
                                message: format!("write prompt: {err}"),
                                location: FlowErrorLocation::new(None, None, None),
                            },
                        )?;
                    }
                    resolve_cli_text(
                        catalog,
                        locale,
                        "cli.qa.prompt.select_option",
                        "Select option",
                    )
                }
                QuestionKind::Bool => resolve_cli_text(
                    catalog,
                    locale,
                    "cli.qa.prompt.enter_true_false",
                    "Enter true/false",
                ),
                QuestionKind::Number => resolve_cli_text(
                    catalog,
                    locale,
                    "cli.qa.prompt.enter_number",
                    "Enter number",
                ),
                QuestionKind::Text => {
                    resolve_cli_text(catalog, locale, "cli.qa.prompt.enter_text", "Enter text")
                }
                QuestionKind::InlineJson { .. } => {
                    resolve_cli_text(catalog, locale, "cli.qa.prompt.enter_json", "Enter JSON")
                }
                QuestionKind::AssetRef { .. } => {
                    resolve_cli_text(catalog, locale, "cli.qa.prompt.enter_path", "Enter path")
                }
            };
            let default = question
                .default
                .as_ref()
                .and_then(|value| crate::wizard_ops::cbor_default_to_json(&question.id, value));
            let raw = prompt_line(&prompt, default.as_ref(), &mut reader, &mut writer)?;
            let value = if raw.trim().is_empty() {
                if let Some(default) = default.clone() {
                    default
                } else if question.required {
                    writeln!(
                        writer,
                        "{}",
                        resolve_cli_text(
                            catalog,
                            locale,
                            "cli.qa.required_field",
                            "This field is required.",
                        )
                    )
                    .map_err(|err| FlowError::Internal {
                        message: format!("write prompt: {err}"),
                        location: FlowErrorLocation::new(None, None, None),
                    })?;
                    continue;
                } else {
                    Value::Null
                }
            } else {
                match parse_answer(&question.kind, &raw) {
                    Ok(value) => value,
                    Err(err) => {
                        writeln!(writer, "{err}").map_err(|write_err| FlowError::Internal {
                            message: format!("write prompt: {write_err}"),
                            location: FlowErrorLocation::new(None, None, None),
                        })?;
                        continue;
                    }
                }
            };
            if value.is_null() && question.required {
                writeln!(
                    writer,
                    "{}",
                    resolve_cli_text(
                        catalog,
                        locale,
                        "cli.qa.required_field",
                        "This field is required.",
                    )
                )
                .map_err(|err| FlowError::Internal {
                    message: format!("write prompt: {err}"),
                    location: FlowErrorLocation::new(None, None, None),
                })?;
                continue;
            }
            answers.insert(question.id.clone(), value);
            match validate_answers_with_form(&form, &answers, &inline_json_ids, true) {
                Ok(true) => break,
                Ok(false) => {
                    answers.remove(&question.id);
                }
                Err(err) => {
                    // Only re-prompt when the rejected answer is the culprit. Errors that belong
                    // to other questions (e.g. an invalid seed) cannot be fixed by re-asking this
                    // one and would loop forever, so surface them.
                    if !errors_only_for_question(&form, &answers, &inline_json_ids, &question.id) {
                        return Err(err);
                    }
                    answers.remove(&question.id);
                    writeln!(writer, "{err}").map_err(|write_err| FlowError::Internal {
                        message: format!("write prompt: {write_err}"),
                        location: FlowErrorLocation::new(None, None, None),
                    })?;
                }
            }
        }
    }
    writer.flush().map_err(|err| FlowError::Internal {
        message: format!("flush stdout: {err}"),
        location: FlowErrorLocation::new(None, None, None),
    })?;
    Ok(answers)
}

pub fn validate_required(
    spec: &ComponentQaSpec,
    catalog: &I18nCatalog,
    locale: &str,
    answers: &HashMap<String, Value>,
) -> Result<()> {
    let form = component_spec_to_form(spec, catalog, locale);
    let _ = validate_answers_with_form(&form, answers, &inline_json_ids(spec), false)?;
    Ok(())
}

/// True when every validation error produced for `answers` is attributed to `question_id`.
fn errors_only_for_question(
    form: &FormSpec,
    answers: &HashMap<String, Value>,
    inline_json_ids: &HashSet<String>,
    question_id: &str,
) -> bool {
    let value = Value::Object(map_from_answers(answers, inline_json_ids));
    let result = qa_spec::validate(form, &value);
    !result.errors.is_empty()
        && result
            .errors
            .iter()
            .all(|err| err.question_id.as_deref() == Some(question_id))
}

fn validate_answers_with_form(
    form: &FormSpec,
    answers: &HashMap<String, Value>,
    inline_json_ids: &HashSet<String>,
    allow_incomplete: bool,
) -> Result<bool> {
    let value = Value::Object(map_from_answers(answers, inline_json_ids));
    let result = qa_spec::validate(form, &value);
    if result.valid {
        return Ok(true);
    }
    if allow_incomplete && result.errors.is_empty() {
        return Ok(true);
    }
    if !allow_incomplete && !result.missing_required.is_empty() {
        return Err(FlowError::Internal {
            message: format!(
                "missing required answers: {}",
                result.missing_required.join(", ")
            ),
            location: FlowErrorLocation::new(None, None, None),
        });
    }
    if !result.errors.is_empty() {
        let lines: Vec<String> = result
            .errors
            .iter()
            .map(|err| err.message.clone())
            .collect();
        return Err(FlowError::Internal {
            message: format!("answers failed validation: {}", lines.join("; ")),
            location: FlowErrorLocation::new(None, None, None),
        });
    }
    Ok(false)
}

/// Ids of questions whose answers are arbitrary JSON. `qa_spec` has no JSON question type, so these
/// are modelled as strings in the form; their values are stringified before validation so the
/// type check does not reject objects/arrays/numbers.
fn inline_json_ids(spec: &ComponentQaSpec) -> HashSet<String> {
    spec.questions
        .iter()
        .filter(|question| matches!(question.kind, QuestionKind::InlineJson { .. }))
        .map(|question| question.id.clone())
        .collect()
}

fn map_from_answers(
    answers: &HashMap<String, Value>,
    inline_json_ids: &HashSet<String>,
) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in answers {
        if value.is_null() {
            continue;
        }
        if inline_json_ids.contains(key) && !value.is_string() {
            map.insert(key.clone(), Value::String(value.to_string()));
        } else {
            map.insert(key.clone(), value.clone());
        }
    }
    map
}

fn component_spec_to_form(spec: &ComponentQaSpec, catalog: &I18nCatalog, locale: &str) -> FormSpec {
    let title = resolve_text(&spec.title, catalog, locale);
    let description = spec
        .description
        .as_ref()
        .map(|text| resolve_text(text, catalog, locale));
    let mut questions = Vec::new();
    for question in &spec.questions {
        let title = resolve_text(&question.label, catalog, locale);
        let description = question
            .help
            .as_ref()
            .map(|text| resolve_text(text, catalog, locale));
        let (kind, choices) = match &question.kind {
            QuestionKind::Text => (QuestionType::String, None),
            QuestionKind::Number => (QuestionType::Number, None),
            QuestionKind::Bool => (QuestionType::Boolean, None),
            QuestionKind::InlineJson { .. } => (QuestionType::String, None),
            QuestionKind::AssetRef { .. } => (QuestionType::String, None),
            QuestionKind::Choice { options } => {
                let list = options.iter().map(|opt| opt.value.clone()).collect();
                (QuestionType::Enum, Some(list))
            }
        };
        let default_value = question
            .default
            .as_ref()
            .and_then(|value| crate::wizard_ops::cbor_default_to_json(&question.id, value))
            .map(|value| value.to_string());
        questions.push(QuestionSpec {
            id: question.id.clone(),
            kind,
            title,
            title_i18n: None,
            description,
            description_i18n: None,
            required: question.required,
            choices,
            default_value,
            secret: false,
            visible_if: None,
            constraint: None,
            list: None,
            computed: None,
            policy: Default::default(),
            computed_overridable: false,
        });
    }
    FormSpec {
        id: "component-setup".to_string(),
        title,
        version: "0.6.0".to_string(),
        description,
        presentation: None,
        progress_policy: None,
        secrets_policy: None,
        store: Vec::new(),
        validations: Vec::new(),
        includes: Vec::new(),
        questions,
    }
}

fn parse_answer(kind: &QuestionKind, raw: &str) -> Result<Value> {
    let trimmed = raw.trim();
    match kind {
        QuestionKind::Text => Ok(Value::String(trimmed.to_string())),
        QuestionKind::Number => {
            if let Ok(int) = trimmed.parse::<i64>() {
                return Ok(Value::Number(Number::from(int)));
            }
            let number: f64 = trimmed.parse().map_err(|err| FlowError::Internal {
                message: format!("invalid number: {err}"),
                location: FlowErrorLocation::new(None, None, None),
            })?;
            Number::from_f64(number)
                .map(Value::Number)
                .ok_or_else(|| FlowError::Internal {
                    message: "number out of range".to_string(),
                    location: FlowErrorLocation::new(None, None, None),
                })
        }
        QuestionKind::Bool => match trimmed.to_ascii_lowercase().as_str() {
            "true" | "t" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
            "false" | "f" | "no" | "n" | "0" => Ok(Value::Bool(false)),
            _ => Err(FlowError::Internal {
                message: "invalid boolean (expected true/false, yes/no, y/n, 1/0)".to_string(),
                location: FlowErrorLocation::new(None, None, None),
            }),
        },
        QuestionKind::InlineJson { .. } => {
            serde_json::from_str(trimmed).map_err(|err| FlowError::Internal {
                message: format!("invalid json: {err}"),
                location: FlowErrorLocation::new(None, None, None),
            })
        }
        QuestionKind::AssetRef { .. } => Ok(Value::String(trimmed.to_string())),
        QuestionKind::Choice { options } => {
            // An exact option value wins over a numeric index so options like ["2", "1"]
            // stay addressable by their literal value.
            let literal = options.iter().any(|opt| opt.value == trimmed);
            if !literal
                && let Ok(idx) = trimmed.parse::<usize>()
                && idx > 0
                && idx <= options.len()
            {
                return Ok(Value::String(options[idx - 1].value.clone()));
            }
            let matched = options
                .iter()
                .find(|opt| opt.value == trimmed)
                .map(|opt| opt.value.clone())
                .ok_or_else(|| FlowError::Internal {
                    message: "invalid choice".to_string(),
                    location: FlowErrorLocation::new(None, None, None),
                })?;
            Ok(Value::String(matched))
        }
    }
}

fn prompt_line<R: BufRead, W: Write>(
    prompt: &str,
    default: Option<&Value>,
    reader: &mut R,
    writer: &mut W,
) -> Result<String> {
    let mut line = String::new();
    if let Some(default) = default {
        write!(writer, "{prompt} [{default}]: ").map_err(|err| FlowError::Internal {
            message: format!("write prompt: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        })?;
    } else {
        write!(writer, "{prompt}: ").map_err(|err| FlowError::Internal {
            message: format!("write prompt: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        })?;
    }
    writer.flush().map_err(|err| FlowError::Internal {
        message: format!("flush stdout: {err}"),
        location: FlowErrorLocation::new(None, None, None),
    })?;
    let bytes_read = reader
        .read_line(&mut line)
        .map_err(|err| FlowError::Internal {
            message: format!("read input: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        })?;
    if bytes_read == 0 {
        return Err(FlowError::Internal {
            message: "read input: unexpected EOF".to_string(),
            location: FlowErrorLocation::new(None, None, None),
        });
    }
    Ok(line.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::i18n_text::I18nText;
    use greentic_types::schemas::component::v0_6_0::{ChoiceOption, QaMode, Question};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    fn text(key: &str, fallback: &str) -> I18nText {
        I18nText::new(key, Some(fallback.to_string()))
    }

    fn question(
        id: &str,
        kind: QuestionKind,
        required: bool,
        default: Option<ciborium::value::Value>,
    ) -> Question {
        Question {
            id: id.to_string(),
            label: text(id, id),
            help: None,
            error: None,
            kind,
            required,
            default,
            skip_if: None,
        }
    }

    fn spec(questions: Vec<Question>) -> ComponentQaSpec {
        ComponentQaSpec {
            mode: QaMode::Default,
            title: text("title", "Setup"),
            description: Some(text("description", "Describe setup")),
            questions,
            defaults: BTreeMap::new(),
        }
    }

    #[test]
    fn validate_required_reports_missing_answers() {
        let spec = spec(vec![question("name", QuestionKind::Text, true, None)]);
        let err = validate_required(&spec, &I18nCatalog::default(), "en", &HashMap::new())
            .expect_err("missing answer should fail");
        assert!(format!("{err}").contains("missing required answers: name"));
    }

    #[test]
    fn run_interactive_with_io_reprompts_until_choice_is_valid() {
        let spec = spec(vec![question(
            "color",
            QuestionKind::Choice {
                options: vec![
                    ChoiceOption {
                        value: "red".to_string(),
                        label: text("red", "Red"),
                    },
                    ChoiceOption {
                        value: "blue".to_string(),
                        label: text("blue", "Blue"),
                    },
                ],
            },
            true,
            None,
        )]);

        let input = Cursor::new("9\n2\n");
        let mut output = Vec::new();
        let answers = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            HashMap::new(),
            input,
            &mut output,
        )
        .expect("interactive answers");

        assert_eq!(answers.get("color"), Some(&json!("blue")));
        let rendered = String::from_utf8(output).expect("utf8");
        assert!(rendered.contains("invalid choice"));
    }

    #[test]
    fn run_interactive_with_io_uses_defaults_and_collects_followup_answers() {
        let spec = spec(vec![
            question(
                "enabled",
                QuestionKind::Bool,
                true,
                Some(ciborium::value::Value::Bool(true)),
            ),
            question("name", QuestionKind::Text, true, None),
        ]);

        let input = Cursor::new("\nAda\n");
        let mut output = Vec::new();
        let answers = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            HashMap::new(),
            input,
            &mut output,
        )
        .expect("interactive answers");

        assert_eq!(answers.get("enabled"), Some(&json!(true)));
        assert_eq!(answers.get("name"), Some(&json!("Ada")));
    }

    #[test]
    fn run_interactive_with_io_reports_unexpected_eof() {
        let spec = spec(vec![question("name", QuestionKind::Text, true, None)]);
        let err = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            HashMap::new(),
            Cursor::new(Vec::<u8>::new()),
            Vec::new(),
        )
        .expect_err("eof should fail");
        assert!(format!("{err}").contains("unexpected EOF"));
    }

    #[test]
    fn parse_answer_covers_number_asset_and_bool_variants() {
        assert_eq!(
            parse_answer(&QuestionKind::Number, "4.5").unwrap(),
            json!(4.5)
        );
        assert_eq!(
            parse_answer(
                &QuestionKind::AssetRef {
                    file_types: Vec::new(),
                    base_path: None,
                    check_exists: true,
                    allow_remote: false,
                },
                " ./asset.json "
            )
            .unwrap(),
            json!("./asset.json")
        );
        assert_eq!(
            parse_answer(&QuestionKind::Bool, "YES").unwrap(),
            json!(true)
        );
        assert_eq!(
            parse_answer(&QuestionKind::Bool, "no").unwrap(),
            json!(false)
        );
    }

    #[test]
    fn parse_answer_rejects_invalid_bool() {
        assert!(parse_answer(&QuestionKind::Bool, "maybe").is_err());
        assert!(parse_answer(&QuestionKind::Bool, "tru").is_err());
        assert_eq!(
            parse_answer(&QuestionKind::Bool, "f").unwrap(),
            json!(false)
        );
    }

    #[test]
    fn run_interactive_with_io_reprompts_on_invalid_bool() {
        let spec = spec(vec![question("enabled", QuestionKind::Bool, true, None)]);
        let mut output = Vec::new();
        let answers = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            HashMap::new(),
            Cursor::new("maybe\nno\n"),
            &mut output,
        )
        .expect("interactive answers");
        assert_eq!(answers.get("enabled"), Some(&json!(false)));
        assert!(
            String::from_utf8(output)
                .expect("utf8")
                .contains("invalid boolean")
        );
    }

    #[test]
    fn run_interactive_with_io_accepts_inline_json_object() {
        let spec = spec(vec![
            question(
                "payload",
                QuestionKind::InlineJson { schema: None },
                true,
                None,
            ),
            question("name", QuestionKind::Text, true, None),
        ]);
        let answers = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            HashMap::new(),
            Cursor::new("{\"a\": [1, 2]}\nAda\n"),
            Vec::new(),
        )
        .expect("inline json must not fail type validation");
        assert_eq!(answers.get("payload"), Some(&json!({"a": [1, 2]})));
        assert_eq!(answers.get("name"), Some(&json!("Ada")));
    }

    #[test]
    fn validate_required_accepts_inline_json_non_string_answers() {
        let spec = spec(vec![question(
            "payload",
            QuestionKind::InlineJson { schema: None },
            true,
            None,
        )]);
        let mut answers = HashMap::new();
        answers.insert("payload".to_string(), json!({"k": 1}));
        validate_required(&spec, &I18nCatalog::default(), "en", &answers)
            .expect("object answer for inline json is valid");
        answers.insert("payload".to_string(), json!(42));
        validate_required(&spec, &I18nCatalog::default(), "en", &answers)
            .expect("number answer for inline json is valid");
    }

    #[test]
    fn run_interactive_with_io_fails_on_invalid_seed_instead_of_looping() {
        let spec = spec(vec![
            question("enabled", QuestionKind::Bool, false, None),
            question("name", QuestionKind::Text, true, None),
        ]);
        let mut seed = HashMap::new();
        seed.insert("enabled".to_string(), json!("not-a-bool"));
        let err = run_interactive_with_io(
            &spec,
            &I18nCatalog::default(),
            "en",
            seed,
            Cursor::new(
                "Ada
Ada
Ada
",
            ),
            Vec::new(),
        )
        .expect_err("invalid seed must surface as an error");
        let message = format!("{err}");
        assert!(
            message.contains("answers failed validation"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn parse_answer_number_keeps_integers_and_literal_choice_wins() {
        let parsed = parse_answer(&QuestionKind::Number, "5").expect("int");
        assert!(parsed.is_i64(), "{parsed:?}");
        let parsed = parse_answer(&QuestionKind::Number, "5.5").expect("float");
        assert_eq!(parsed, json!(5.5));

        let kind = QuestionKind::Choice {
            options: vec![
                ChoiceOption {
                    value: "2".to_string(),
                    label: text("a", "A"),
                },
                ChoiceOption {
                    value: "1".to_string(),
                    label: text("b", "B"),
                },
            ],
        };
        assert_eq!(parse_answer(&kind, "2").expect("literal"), json!("2"));
    }
}
