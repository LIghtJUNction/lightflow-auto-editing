use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_work_subtitles";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Video Work Subtitle Extraction",
        description: "Rust-native authenticated Video Work API MCP subtitle extraction.",
        input "video_path": "path" {
            description: "Video Work API sandboxed video path.",
            required: true,
            widget: "file",
        }
        output "subtitles": "json" { description: "Video Work API subtitle result and timing evidence." }
        output "summary": "text" { description: "Rust-native workflow summary." }
    }.builtin_runtime("command", "lightflow.command.run", "runner.v1").build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, VideoWorkError> {
    let video_path = inputs
        .get("video_path")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| VideoWorkError::new("video_path must be non-empty"))?;
    let subtitles = lightflow_video_work_mcp::call(
        "extract_video_subtitles",
        Map::from_iter([("video_path".to_owned(), video_path.into())]),
    )
    .map_err(VideoWorkError::from)?;
    validate_subtitles(&subtitles)?;
    Ok(Response {
        outputs: Map::from_iter([
            ("subtitles".to_owned(), subtitles.into()),
            (
                "summary".to_owned(),
                "Subtitles extracted by Rust-native Video Work API workflow.".into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([(
            "implementation".to_owned(),
            implementation_identity().into(),
        )]),
    })
}

fn validate_subtitles(subtitles: &Map<String, Value>) -> Result<(), VideoWorkError> {
    for (field, valid) in [
        (
            "segments",
            subtitles.get("segments").is_some_and(Value::is_array),
        ),
        ("srt", subtitles.get("srt").is_some_and(Value::is_string)),
        ("words", subtitles.get("words").is_some_and(Value::is_array)),
    ] {
        if !valid {
            return Err(VideoWorkError::new(format!(
                "subtitle payload {field} has an invalid type"
            )));
        }
    }
    Ok(())
}

fn implementation_identity() -> String {
    format!(
        "lightflow.video_work_subtitles.rust.fnv1a64:{:016x}",
        digest(include_bytes!("lib.rs"))
    )
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
pub struct VideoWorkError(String);
impl VideoWorkError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl From<lightflow_video_work_mcp::Error> for VideoWorkError {
    fn from(value: lightflow_video_work_mcp::Error) -> Self {
        Self(value.to_string())
    }
}
impl std::fmt::Display for VideoWorkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for VideoWorkError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::json;

    #[test]
    fn accepts_subtitle_payload_contract() {
        let payload = json!({"segments": [], "srt": "", "words": []});
        assert!(validate_subtitles(payload.as_object().unwrap()).is_ok());
    }

    #[test]
    fn rejects_invalid_subtitle_payload_contract() {
        for payload in [
            json!({"srt": "text", "words": []}),
            json!({"segments": {}, "srt": "text", "words": []}),
            json!({"segments": [], "srt": [], "words": []}),
            json!({"segments": [], "srt": "text", "words": {}}),
        ] {
            assert!(validate_subtitles(payload.as_object().unwrap()).is_err());
        }
    }
}
