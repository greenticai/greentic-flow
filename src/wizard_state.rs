use crate::error::{FlowError, FlowErrorLocation, Result};
use greentic_types::cbor::canonical;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardState {
    pub flow_id: String,
    pub locale: String,
    pub steps: Vec<WizardStepState>,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardStepState {
    pub node_id: String,
    pub mode: String,
    pub updated_at: u64,
}

/// Makes a flow id safe to use as a single file-name component: every character outside
/// `[A-Za-z0-9._-]` is percent-escaped, and `.`/`..`/empty ids are escaped entirely, so
/// an id read from an untrusted flow document can never traverse out of the cache dir.
fn sanitize_flow_id_for_file_name(flow_id: &str) -> String {
    if matches!(flow_id, "" | "." | "..") {
        return flow_id
            .bytes()
            .map(|b| format!("%{b:02X}"))
            .collect::<String>()
            + "%00";
    }
    let mut out = String::with_capacity(flow_id.len());
    for byte in flow_id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn wizard_state_path(flow_path: &Path, flow_id: &str) -> PathBuf {
    let base = flow_path.parent().unwrap_or_else(|| Path::new("."));
    base.join(".greentic/cache/flow_wizard")
        .join(format!("{}.cbor", sanitize_flow_id_for_file_name(flow_id)))
}

/// Why loading the wizard state cache failed; lets callers tell a corrupt cache from I/O errors.
enum LoadError {
    Read(FlowError),
    Decode(String),
}

fn read_wizard_state(
    flow_path: &Path,
    flow_id: &str,
) -> std::result::Result<Option<WizardState>, LoadError> {
    let path = wizard_state_path(flow_path, flow_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|err| {
        LoadError::Read(FlowError::Internal {
            message: format!("read wizard state: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        })
    })?;
    let state: WizardState =
        canonical::from_cbor(&bytes).map_err(|err| LoadError::Decode(err.to_string()))?;
    Ok(Some(state))
}

pub fn load_wizard_state(flow_path: &Path, flow_id: &str) -> Result<Option<WizardState>> {
    match read_wizard_state(flow_path, flow_id) {
        Ok(state) => Ok(state),
        Err(LoadError::Read(err)) => Err(err),
        Err(LoadError::Decode(message)) => Err(FlowError::Internal {
            message: format!("decode wizard state: {message}"),
            location: FlowErrorLocation::new(None, None, None),
        }),
    }
}

/// The wizard state is a disposable cache: an undecodable file (e.g. truncated by an
/// interrupted write) is discarded so mutating commands self-heal instead of failing forever.
fn load_or_reset_wizard_state(flow_path: &Path, flow_id: &str) -> Result<Option<WizardState>> {
    match read_wizard_state(flow_path, flow_id) {
        Ok(state) => Ok(state),
        Err(LoadError::Decode(message)) => {
            eprintln!(
                "warning: discarding corrupt wizard state cache (decode wizard state: {message})"
            );
            Ok(None)
        }
        Err(LoadError::Read(err)) => Err(err),
    }
}

pub fn update_wizard_state(
    flow_path: &Path,
    flow_id: &str,
    node_id: &str,
    mode: &str,
    locale: &str,
) -> Result<()> {
    let mut state =
        load_or_reset_wizard_state(flow_path, flow_id)?.unwrap_or_else(|| WizardState {
            flow_id: flow_id.to_string(),
            locale: locale.to_string(),
            steps: Vec::new(),
            last_updated: 0,
        });
    let now = now_epoch_secs();
    state.locale = locale.to_string();
    state.last_updated = now;
    if let Some(step) = state.steps.iter_mut().find(|s| s.node_id == node_id) {
        step.mode = mode.to_string();
        step.updated_at = now;
    } else {
        state.steps.push(WizardStepState {
            node_id: node_id.to_string(),
            mode: mode.to_string(),
            updated_at: now,
        });
    }
    write_wizard_state(flow_path, &state)
}

pub fn remove_wizard_step(flow_path: &Path, flow_id: &str, node_id: &str) -> Result<()> {
    let Some(mut state) = load_or_reset_wizard_state(flow_path, flow_id)? else {
        return Ok(());
    };
    let now = now_epoch_secs();
    state.last_updated = now;
    state.steps.retain(|step| step.node_id != node_id);
    write_wizard_state(flow_path, &state)
}

fn write_wizard_state(flow_path: &Path, state: &WizardState) -> Result<()> {
    let path = wizard_state_path(flow_path, &state.flow_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| FlowError::Internal {
            message: format!("create wizard state directory: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        })?;
    }
    let bytes = canonical::to_canonical_cbor(state).map_err(|err| FlowError::Internal {
        message: format!("encode wizard state: {err}"),
        location: FlowErrorLocation::new(None, None, None),
    })?;
    // Write to a sibling temp file then rename so readers never observe a partial file.
    // The name is unique per write (pid + counter) so concurrent writers never share a temp file.
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("cbor.tmp-{}-{seq}", std::process::id()));
    let result = write_synced(&tmp, &bytes).and_then(|()| fs::rename(&tmp, &path));
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp);
        return Err(FlowError::Internal {
            message: format!("write wizard state: {err}"),
            location: FlowErrorLocation::new(None, None, None),
        });
    }
    Ok(())
}

/// Writes and fsyncs so a power loss after the rename cannot leave an empty/truncated file.
fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_writes_do_not_collide_and_corrupt_cache_heals() {
        let dir = tempfile::tempdir().expect("tempdir");
        let flow = dir.path().join("a.ygtc");
        std::thread::scope(|scope| {
            for i in 0..8 {
                let flow = &flow;
                scope.spawn(move || {
                    for j in 0..20 {
                        update_wizard_state(flow, "f", &format!("n{i}-{j}"), "default", "en")
                            .expect("concurrent write");
                    }
                });
            }
        });
        assert!(load_wizard_state(&flow, "f").expect("load").is_some());
        let path = wizard_state_path(&flow, "f");
        fs::write(&path, b"\xff\xff garbage").expect("corrupt");
        assert!(load_wizard_state(&flow, "f").is_err());
        update_wizard_state(&flow, "f", "n", "default", "en").expect("self-heal");
        assert_eq!(
            load_wizard_state(&flow, "f")
                .expect("load")
                .expect("some")
                .steps
                .len(),
            1
        );
    }

    #[test]
    fn state_path_never_escapes_cache_dir() {
        let flow = Path::new("/pack/flows/a.ygtc");
        let base = Path::new("/pack/flows/.greentic/cache/flow_wizard");
        for id in ["../../x", "a/b", "..", ".", "", "a\\b"] {
            let path = wizard_state_path(flow, id);
            assert_eq!(path.parent(), Some(base), "id {id:?} -> {path:?}");
            assert!(
                path.components()
                    .all(|c| !matches!(c, std::path::Component::ParentDir)),
                "{path:?}"
            );
        }
        assert_eq!(
            wizard_state_path(flow, "plain-id_1.2"),
            base.join("plain-id_1.2.cbor")
        );
    }
}
