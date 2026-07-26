//! Rust-native execution boundary for the automatic-video LightFlow workflows.

mod cover;
mod evidence;
mod media;
mod plan;
mod render;
mod subtitles;

use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const PLAN_WORKFLOW: &str = "lightflow.video_auto_edit_plan";
pub const RENDER_WORKFLOW: &str = "lightflow.video_render_edit";
pub const AUTO_EDIT_WORKFLOW: &str = "lightflow.video_auto_edit";
pub const COVER_WORKFLOW: &str = "lightflow.video_cover_image";
pub const SUBTITLES_WORKFLOW: &str = "lightflow.video_subtitles";

/// Execute a package workflow without embedding or invoking a secondary runtime.
pub fn execute(
    workflow_id: &str,
    _workflow_version: &str,
    inputs: &Map<String, Value>,
    leaf_identity: &str,
) -> Result<Response, RuntimeError> {
    let base_dir = std::env::current_dir().map_err(RuntimeError::io)?;
    let mut response = match workflow_id {
        PLAN_WORKFLOW => plan::execute(inputs, &base_dir),
        RENDER_WORKFLOW => render::execute(inputs, &base_dir),
        AUTO_EDIT_WORKFLOW => {
            let planned = plan::build(inputs, &base_dir)?;
            render::execute_plan(&planned.plan, &planned.summary, inputs, &base_dir)
        }
        COVER_WORKFLOW => cover::execute(inputs, &base_dir),
        SUBTITLES_WORKFLOW => subtitles::execute(inputs, &base_dir),
        _ => Err(RuntimeError::new(format!(
            "unsupported workflow id {workflow_id:?}"
        ))),
    }?;
    response.replay_fingerprint.insert(
        "implementation".to_owned(),
        format!(
            "lightflow.auto_edit.rust.fnv1a64:{:016x}",
            digest(leaf_identity.as_bytes())
        )
        .into(),
    );
    Ok(response)
}

const fn digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
        index += 1;
    }
    hash
}

#[derive(Debug)]
pub struct RuntimeError(String);

impl RuntimeError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub(crate) fn io(error: std::io::Error) -> Self {
        Self(format!("media I/O failed: {error}"))
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_workflow() {
        assert!(execute("unknown", "0", &Map::new(), "leaf").is_err());
    }
}
