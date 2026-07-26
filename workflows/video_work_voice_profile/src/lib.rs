use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_work_voice_profile";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
    name: "Video Work Voice Profile", description: "Rust-native consent-gated Video Work API voice-reference import.",
    input "speaker_id": "text" { description: "Existing Video Work API speaker ID.", required: true, widget: "text", }
    input "style_name": "text" { description: "Profile style name.", required: true, widget: "text", }
    input "prompt_text": "text" { description: "Exact reference transcript.", required: true, widget: "textarea", }
    input "audio_path": "path" { description: "Video Work API sandboxed reference-audio path.", required: true, widget: "file", }
    input "confirm_rights": "boolean" { description: "Must be true only after explicit informed rights confirmation.", required: true, widget: "checkbox", }
    output "voice_profile": "json" { description: "Created Video Work API voice profile." }
    output "summary": "text" { description: "Rust-native workflow summary." }
}.builtin_runtime("command", "lightflow.command.run", "runner.v1").build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, VoiceError> {
    let (action, arguments) = build_request(inputs)?;
    let voice_profile =
        lightflow_video_work_mcp::call(action, arguments).map_err(VoiceError::from)?;
    validate_voice_profile(&voice_profile)?;
    Ok(Response {
        outputs: Map::from_iter([
            ("voice_profile".to_owned(), voice_profile.into()),
            (
                "summary".to_owned(),
                "Voice profile imported by Rust-native Video Work API workflow.".into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([(
            "implementation".to_owned(),
            implementation_identity().into(),
        )]),
    })
}

fn build_request(
    inputs: &Map<String, Value>,
) -> Result<(&'static str, Map<String, Value>), VoiceError> {
    if inputs.get("confirm_rights") != Some(&Value::Bool(true)) {
        return Err(VoiceError::new(
            "confirm_rights must be true before importing a voice reference",
        ));
    }
    let mut arguments = Map::new();
    for name in ["speaker_id", "style_name", "prompt_text", "audio_path"] {
        let value = inputs
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| VoiceError::new("voice-profile text input must be non-empty"))?;
        arguments.insert(name.to_owned(), value.trim().into());
    }
    arguments.insert("confirm_rights".to_owned(), Value::Bool(true));
    Ok(("add_voice_profile", arguments))
}

fn validate_voice_profile(voice_profile: &Map<String, Value>) -> Result<(), VoiceError> {
    for field in ["id", "speaker_id", "style_name"] {
        voice_profile
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| VoiceError::new(format!("voice profile {field} must be non-empty")))?;
    }
    voice_profile
        .get("duration_seconds")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| VoiceError::new("voice profile duration_seconds must be positive"))?;
    Ok(())
}

fn implementation_identity() -> String {
    format!(
        "lightflow.video_work_voice_profile.rust.fnv1a64:{:016x}",
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
pub struct VoiceError(String);
impl VoiceError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl From<lightflow_video_work_mcp::Error> for VoiceError {
    fn from(value: lightflow_video_work_mcp::Error) -> Self {
        Self(value.to_string())
    }
}
impl std::fmt::Display for VoiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for VoiceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::json;

    #[test]
    fn accepts_complete_voice_profile_payload() {
        let payload = json!({
            "id": "profile-1",
            "speaker_id": "speaker-1",
            "style_name": "Narration",
            "duration_seconds": 4.2
        });
        assert!(validate_voice_profile(payload.as_object().unwrap()).is_ok());
    }

    #[test]
    fn rejects_incomplete_voice_profile_payload() {
        for payload in [
            json!({"speaker_id": "speaker-1", "style_name": "Narration", "duration_seconds": 4.2}),
            json!({"id": "profile-1", "speaker_id": "", "style_name": "Narration", "duration_seconds": 4.2}),
            json!({"id": "profile-1", "speaker_id": "speaker-1", "style_name": "Narration", "duration_seconds": 0}),
        ] {
            assert!(validate_voice_profile(payload.as_object().unwrap()).is_err());
        }
    }

    #[test]
    fn request_includes_only_the_confirmed_service_contract() {
        let inputs = Map::from_iter([
            ("speaker_id".to_owned(), json!(" speaker-1 ")),
            ("style_name".to_owned(), json!(" Narration ")),
            ("prompt_text".to_owned(), json!(" Exact transcript ")),
            ("audio_path".to_owned(), json!(" reference.wav ")),
            ("confirm_rights".to_owned(), json!(true)),
            ("unrelated".to_owned(), json!("must not leave the workflow")),
        ]);

        let (action, arguments) = build_request(&inputs).unwrap();
        assert_eq!(action, "add_voice_profile");
        assert_eq!(arguments.len(), 5);
        assert_eq!(arguments.get("speaker_id"), Some(&json!("speaker-1")));
        assert_eq!(arguments.get("style_name"), Some(&json!("Narration")));
        assert_eq!(
            arguments.get("prompt_text"),
            Some(&json!("Exact transcript"))
        );
        assert_eq!(arguments.get("audio_path"), Some(&json!("reference.wav")));
        assert_eq!(arguments.get("confirm_rights"), Some(&json!(true)));
    }
}
