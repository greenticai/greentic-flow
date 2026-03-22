#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArtifactKey {
    pub engine_profile_id: String,
    pub wasm_digest: String,
}

impl ArtifactKey {
    pub fn new(engine_profile_id: String, wasm_digest: String) -> Self {
        Self {
            engine_profile_id,
            wasm_digest,
        }
    }
}
