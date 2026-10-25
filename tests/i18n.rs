use assert_cmd::cargo::cargo_bin_cmd;
use greentic_flow::i18n::{I18nCatalog, locale_fallback_chain, resolve_locale, resolve_text};
use greentic_types::i18n_text::I18nText;
use predicates::str::contains;
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

const LOCALE_VARS: [&str; 4] = ["GREENTIC_LOCALE", "LC_ALL", "LC_MESSAGES", "LANG"];

/// Serializes env-mutating tests, clears all locale vars, and restores the originals on drop
/// (including when an assertion panics).
struct LocaleEnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl LocaleEnvGuard {
    fn new() -> Self {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = LOCALE_VARS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        let guard = Self { _lock: lock, saved };
        for key in LOCALE_VARS {
            guard.unset(key);
        }
        guard
    }

    fn set(&self, key: &str, value: &str) {
        // SAFETY: env mutation is serialized by the guard's mutex within this test binary.
        unsafe { std::env::set_var(key, value) };
    }

    fn unset(&self, key: &str) {
        // SAFETY: see `set`.
        unsafe { std::env::remove_var(key) };
    }
}

impl Drop for LocaleEnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            // SAFETY: the lock is still held while fields drop after this body.
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

#[test]
fn locale_chain_falls_back_to_language_and_en() {
    let chain = locale_fallback_chain("nl-NL");
    assert_eq!(chain, vec!["nl-NL", "nl", "en"]);
}

#[test]
fn resolve_locale_prefers_explicit_then_env_vars() {
    let env = LocaleEnvGuard::new();
    assert_eq!(resolve_locale(Some("pt-BR")), "pt-BR");

    env.set("LC_ALL", "fr_FR.UTF-8");
    assert_eq!(resolve_locale(None), "fr-FR");
    // explicit still wins over env
    assert_eq!(resolve_locale(Some("pt-BR")), "pt-BR");

    env.unset("LC_ALL");
    env.set("LANG", "nl_NL.UTF-8");
    assert_eq!(resolve_locale(None), "nl-NL");
}

#[test]
fn resolve_locale_prefers_lc_all_over_other_system_vars() {
    let env = LocaleEnvGuard::new();
    env.set("LC_ALL", "de_DE.UTF-8");
    env.set("LC_MESSAGES", "es_ES.UTF-8");
    env.set("LANG", "nl_NL.UTF-8");
    assert_eq!(resolve_locale(None), "de-DE");
}

/// With every env locale var cleared, resolution falls through to the OS locale (`sys_locale`)
/// and finally to "en". The OS value is machine-dependent, so only the invariant is asserted:
/// a non-empty, well-formed tag whose fallback chain always ends in "en".
#[test]
fn resolve_locale_without_env_vars_yields_valid_locale_or_en() {
    let _env = LocaleEnvGuard::new();
    let locale = resolve_locale(None);
    assert!(!locale.is_empty());
    assert!(
        locale
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "unexpected locale shape: {locale}"
    );
    assert_eq!(
        locale_fallback_chain(&locale).last().map(String::as_str),
        Some("en")
    );
}

#[test]
fn resolve_locale_keeps_greentic_override_for_compat() {
    let env = LocaleEnvGuard::new();
    env.set("GREENTIC_LOCALE", "es_MX.UTF-8");
    env.set("LC_ALL", "de_DE.UTF-8");
    assert_eq!(resolve_locale(None), "es-MX");
}

#[test]
fn resolve_text_prefers_catalog_then_fallback_then_key() {
    let mut catalog = I18nCatalog::default();
    catalog.insert("greeting", "nl", "Hallo".to_string());
    catalog.insert("greeting", "en", "Hello".to_string());

    let text = I18nText::new("greeting", Some("Hi".to_string()));
    assert_eq!(resolve_text(&text, &catalog, "nl-NL"), "Hallo");

    let text = I18nText::new("missing", Some("Fallback".to_string()));
    assert_eq!(resolve_text(&text, &catalog, "nl-NL"), "Fallback");

    let text = I18nText::new("missing2", None);
    assert_eq!(resolve_text(&text, &catalog, "nl-NL"), "missing2");
}

#[test]
fn cli_help_uses_requested_non_english_locale() {
    cargo_bin_cmd!("greentic-flow")
        .arg("--locale")
        .arg("es")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Ayudantes de andamiaje de flujos"));
}
