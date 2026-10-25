use handlebars::{
    Context, Handlebars, Helper, HelperDef, Output, RenderContext, RenderError, RenderErrorReason,
    Renderable,
};
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::{Map, Value};

use crate::error::{FlowError, FlowErrorLocation, Result};

/// Token delimiters are private-use code points: they cannot appear in a state
/// key, and the token body is a decimal index into a side table, so adjacent
/// placeholders or identifier characters (`_`, digits) after a token can never
/// be absorbed into it.
const STATE_TOKEN_OPEN: char = '\u{E000}';
const STATE_TOKEN_CLOSE: char = '\u{E001}';

lazy_static! {
    /// `{{state.key}}` placeholders that are rewritten to tokens before rendering.
    /// Compiled once; the pattern is constant so `None` is unreachable in practice.
    static ref STATE_PLACEHOLDER_RE: Option<Regex> =
        Regex::new(r"\{\{\s*state\.([A-Za-z_]\w*)\s*\}\}").ok();
    /// Tokens produced by [`state_token_value`] anywhere inside a string.
    static ref STATE_TOKEN_RE: Option<Regex> = Regex::new(&format!(
        r"{}(\d+){}",
        regex::escape(&STATE_TOKEN_OPEN.to_string()),
        regex::escape(&STATE_TOKEN_CLOSE.to_string())
    ))
    .ok();
}

/// Escape a rendered value so it is safe inside a JSON string literal.
fn json_string_escape(s: &str) -> String {
    match serde_json::to_string(s) {
        Ok(quoted) => quoted
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .map(str::to_string)
            .unwrap_or(quoted),
        Err(_) => s.to_string(),
    }
}

pub struct TemplateRenderer {
    handlebars: Handlebars<'static>,
    manifest_id: Option<String>,
}

impl TemplateRenderer {
    pub fn new(manifest_id: Option<String>) -> Self {
        let mut handlebars = Handlebars::new();
        handlebars.register_escape_fn(json_string_escape);
        handlebars.register_helper("json", Box::new(JsonHelper));
        handlebars.register_helper("default", Box::new(DefaultHelper));
        handlebars.register_helper("ifEq", Box::new(IfEqHelper));
        Self {
            handlebars,
            manifest_id,
        }
    }

    pub fn render_json(
        &self,
        template: &str,
        state: &Map<String, Value>,
        node_id: &str,
    ) -> Result<Value> {
        let (preprocessed, keys) = preprocess_template(template);
        let mut ctx = Map::new();
        ctx.insert("state".to_string(), Value::Object(state.clone()));
        let rendered = self
            .handlebars
            .render_template(&preprocessed, &ctx)
            .map_err(|e| FlowError::Internal {
                message: format!(
                    "template render error in node '{node_id}'{}: {e}",
                    manifest_label(self.manifest_id.as_deref())
                ),
                location: FlowErrorLocation::at_path(format!("nodes.{node_id}.template")),
            })?;
        let mut value: Value =
            serde_json::from_str(&rendered).map_err(|e| FlowError::Internal {
                message: format!(
                    "template JSON parse error in node '{node_id}'{}: {e}",
                    manifest_label(self.manifest_id.as_deref())
                ),
                location: FlowErrorLocation::at_path(format!("nodes.{node_id}.template")),
            })?;
        substitute_state_tokens(&mut value, state, &keys).map_err(|e| FlowError::Internal {
            message: format!(
                "{e} (node '{node_id}'{})",
                manifest_label(self.manifest_id.as_deref())
            ),
            location: FlowErrorLocation::at_path(format!("nodes.{node_id}.template")),
        })?;
        Ok(value)
    }
}

fn manifest_label(manifest_id: Option<&str>) -> String {
    manifest_id
        .map(|id| format!(" in manifest '{id}'"))
        .unwrap_or_default()
}

/// Rewrite `{{state.key}}` placeholders to index tokens; returns the rewritten
/// template and the key table the tokens index into.
fn preprocess_template(template: &str) -> (String, Vec<String>) {
    let Some(re) = STATE_PLACEHOLDER_RE.as_ref() else {
        return (template.to_string(), Vec::new());
    };
    let mut keys: Vec<String> = Vec::new();
    let rewritten = re
        .replace_all(template, |caps: &regex::Captures<'_>| {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            keys.push(key.to_string());
            state_token_value(keys.len() - 1)
        })
        .to_string();
    (rewritten, keys)
}

fn state_token_value(index: usize) -> String {
    format!("{STATE_TOKEN_OPEN}{index}{STATE_TOKEN_CLOSE}")
}

/// Parse a string that is exactly one token into its key-table index.
fn whole_token_index(s: &str) -> Option<usize> {
    s.strip_prefix(STATE_TOKEN_OPEN)
        .and_then(|rest| rest.strip_suffix(STATE_TOKEN_CLOSE))
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|digits| digits.parse().ok())
}

fn substitute_state_tokens(
    target: &mut Value,
    state: &Map<String, Value>,
    keys: &[String],
) -> std::result::Result<(), String> {
    match target {
        Value::String(s) => {
            let whole_key = whole_token_index(s).and_then(|idx| keys.get(idx));
            if let Some(key) = whole_key {
                // Whole-string token keeps the original JSON type.
                let value = state
                    .get(key)
                    .ok_or_else(|| format!("state value for '{key}' not found"))?;
                *target = value.clone();
            } else if s.contains(STATE_TOKEN_OPEN) {
                *s = interpolate_partial(s, state, keys)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                substitute_state_tokens(item, state, keys)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                substitute_state_tokens(value, state, keys)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Replace every token embedded in a larger string with the textual form of the
/// state value (strings verbatim, other types as compact JSON). Single pass, so
/// substituted values are never re-scanned. Tokens whose index is not in the
/// key table (not produced by this render) are left untouched.
fn interpolate_partial(
    input: &str,
    state: &Map<String, Value>,
    keys: &[String],
) -> std::result::Result<String, String> {
    let Some(re) = STATE_TOKEN_RE.as_ref() else {
        return Ok(input.to_string());
    };
    let mut out = String::with_capacity(input.len());
    let mut last = 0;
    for caps in re.captures_iter(input) {
        let (Some(whole), Some(idx)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        let Some(key) = idx.as_str().parse::<usize>().ok().and_then(|i| keys.get(i)) else {
            continue;
        };
        let value = state
            .get(key)
            .ok_or_else(|| format!("state value for '{key}' not found"))?;
        out.push_str(&input[last..whole.start()]);
        match value {
            Value::String(text) => out.push_str(text),
            other => out.push_str(&other.to_string()),
        }
        last = whole.end();
    }
    out.push_str(&input[last..]);
    Ok(out)
}

struct JsonHelper;

impl HelperDef for JsonHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> std::result::Result<(), RenderError> {
        let value = helper
            .param(0)
            .map(|p| p.value().clone())
            .ok_or_else(|| helper_error("json helper expects 1 parameter"))?;
        let rendered = serde_json::to_string(&value)
            .map_err(|e| helper_error(&format!("json helper: {e}")))?;
        out.write(&rendered)?;
        Ok(())
    }
}

struct DefaultHelper;

impl HelperDef for DefaultHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> std::result::Result<(), RenderError> {
        let value = helper.param(0).map(|p| p.value().clone());
        let fallback = helper
            .param(1)
            .map(|p| p.value().clone())
            .ok_or_else(|| helper_error("default helper expects 2 parameters"))?;
        let use_fallback = matches!(value.as_ref(), None | Some(Value::Null))
            || matches!(value.as_ref(), Some(Value::String(s)) if s.is_empty());
        let rendered_value = match value {
            Some(v) if !use_fallback => v,
            _ => fallback,
        };
        let rendered = serde_json::to_string(&rendered_value)
            .map_err(|e| helper_error(&format!("default helper: {e}")))?;
        out.write(&rendered)?;
        Ok(())
    }
}

struct IfEqHelper;

impl HelperDef for IfEqHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        r: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> std::result::Result<(), RenderError> {
        let left = helper
            .param(0)
            .map(|p| p.value().clone())
            .ok_or_else(|| helper_error("ifEq helper expects 2 parameters"))?;
        let right = helper
            .param(1)
            .map(|p| p.value().clone())
            .ok_or_else(|| helper_error("ifEq helper expects 2 parameters"))?;
        let matches = left == right;
        if matches {
            if let Some(t) = helper.template() {
                t.render(r, ctx, rc, out)?;
            }
        } else if let Some(t) = helper.inverse() {
            t.render(r, ctx, rc, out)?;
        }
        Ok(())
    }
}

fn helper_error(message: &str) -> RenderError {
    RenderErrorReason::Other(message.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(template: &str, state: Map<String, Value>) -> Value {
        let renderer = TemplateRenderer::new(Some("manifest.test".to_string()));
        renderer
            .render_json(template, &state, "emit_config")
            .unwrap()
    }

    #[test]
    fn adjacent_placeholders_interpolate() {
        let mut state = Map::new();
        state.insert("first".to_string(), json!("Ada"));
        state.insert("last".to_string(), json!("Lovelace"));
        let value = render(
            r#"{ "name": "{{state.first}}_{{state.last}}", "both": "{{state.first}}{{state.last}}" }"#,
            state,
        );
        assert_eq!(value["name"], "Ada_Lovelace");
        assert_eq!(value["both"], "AdaLovelace");
    }

    #[test]
    fn underscore_and_digits_after_placeholder_are_literal() {
        let mut state = Map::new();
        state.insert("id".to_string(), json!(7));
        let value = render(
            r#"{ "a": "{{state.id}}_suffix", "b": "{{state.id}}__", "c": "x{{state.id}}9" }"#,
            state,
        );
        assert_eq!(value["a"], "7_suffix");
        assert_eq!(value["b"], "7__");
        assert_eq!(value["c"], "x79");
    }

    #[test]
    fn whole_string_placeholder_keeps_json_type() {
        let mut state = Map::new();
        state.insert("n".to_string(), json!(3));
        state.insert("obj".to_string(), json!({"k": [1, 2]}));
        let value = render(r#"{ "n": "{{state.n}}", "o": "{{ state.obj }}" }"#, state);
        assert_eq!(value["n"], json!(3));
        assert_eq!(value["o"], json!({"k": [1, 2]}));
    }

    #[test]
    fn literal_legacy_token_text_is_not_substituted() {
        let state = Map::new();
        let value = render(r#"{ "a": "__STATE_TOKEN__x__" }"#, state);
        assert_eq!(value["a"], "__STATE_TOKEN__x__");
    }

    #[test]
    fn missing_state_value_errors() {
        let renderer = TemplateRenderer::new(None);
        let err = renderer
            .render_json(r#"{ "a": "{{state.nope}}_x" }"#, &Map::new(), "n")
            .unwrap_err();
        assert!(err.to_string().contains("'nope' not found"), "{err}");
    }

    #[test]
    fn if_truthy_renders_block() {
        let mut state = Map::new();
        state.insert("needs_interaction".to_string(), Value::Bool(true));
        let template = r#"{ "enabled": {{#if state.needs_interaction}}true{{/if}} }"#;
        let value = render(template, state);
        assert_eq!(value.get("enabled"), Some(&Value::Bool(true)));
    }

    #[test]
    fn ifeq_matches_string_and_bool() {
        let mut state = Map::new();
        state.insert("mode".to_string(), Value::String("asset".to_string()));
        state.insert("flag".to_string(), Value::Bool(false));
        let template = r#"
        {
          "mode": {{#ifEq state.mode "asset"}} "asset" {{else}} "inline" {{/ifEq}},
          "flagged": {{#ifEq state.flag false}} true {{else}} false {{/ifEq}}
        }"#;
        let value = render(template, state);
        assert_eq!(value.get("mode"), Some(&Value::String("asset".to_string())));
        assert_eq!(value.get("flagged"), Some(&Value::Bool(true)));
    }

    #[test]
    fn json_helper_emits_raw_json() {
        let mut state = Map::new();
        state.insert("inline_json".to_string(), json!({"a": 1, "b": [true]}));
        let template = r#"{ "inline": {{json state.inline_json}} }"#;
        let value = render(template, state);
        assert_eq!(value.get("inline"), Some(&json!({"a": 1, "b": [true]})));
    }

    #[test]
    fn preserves_simple_state_interpolation() {
        let mut state = Map::new();
        state.insert("temperature".to_string(), json!(0.4));
        let template = r#"{ "temperature": "{{state.temperature}}" }"#;
        let value = render(template, state);
        assert_eq!(value.get("temperature"), Some(&json!(0.4)));
    }

    #[test]
    fn partial_state_interpolation_substitutes_inline() {
        let mut state = Map::new();
        state.insert("name".to_string(), json!("Ada"));
        state.insert("n".to_string(), json!(3));
        let value = render(r#"{ "msg": "Hello {{state.name}}! #{{state.n}}" }"#, state);
        assert_eq!(value.get("msg"), Some(&json!("Hello Ada! #3")));
    }

    #[test]
    fn nested_path_values_are_json_escaped() {
        let mut state = Map::new();
        state.insert(
            "user".to_string(),
            json!({"name": "x\", \"admin\": true, \"y\": \""}),
        );
        let value = render(r#"{ "who": "{{state.user.name}}" }"#, state);
        assert_eq!(
            value.get("who"),
            Some(&json!("x\", \"admin\": true, \"y\": \""))
        );
        assert!(value.get("admin").is_none());
    }
}
