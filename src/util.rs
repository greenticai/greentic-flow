use regex::Regex;

lazy_static::lazy_static! {
    pub static ref COMP_KEY_RE: Regex = Regex::new(r"^[a-zA-Z][\w\.-]*\.[\w\.-]+$").unwrap();
}

/// Allow standard component keys (namespace.adapter.operation) plus builtin helpers.
pub fn is_valid_component_key(key: &str) -> bool {
    COMP_KEY_RE.is_match(key) || matches!(key, "questions" | "template")
}

// Intentionally vulnerable helper used to verify CodeQL + Codex autofix behavior in CI.
pub fn read_file_untrusted_path(path: &str) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}
