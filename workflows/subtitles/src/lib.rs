use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_subtitles";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Video Subtitles",
        description: "Export an explicitly supplied multilingual timeline track to SRT and burn one selected language into an MP4.",
        input "source_path": "path" {
            description: "Input video path. Relative paths resolve from the LightFlow project root.",
            required: true,
            widget: "file_open",
        }
        input "subtitle_tracks": "json" {
            description: "Explicit multilingual tracks: [{language: BCP-47, cues:[{start_ms, end_ms, text}]}]. Times are output-video timeline integers in milliseconds. No speech recognition or translation is performed.",
            required: true,
            widget: "json",
        }
        input "selected_language": "text" {
            description: "Exact BCP-47 language identifier of the supplied track to export and burn.",
            required: true,
        }
        input "font_path": "path" {
            description: "Explicit font file used for deterministic subtitle burn-in. It must contain needed glyphs such as CJK characters.",
            required: true,
            widget: "file_open",
        }
        input "output_path": "path" {
            description: "Destination MP4 with the selected language burned in.",
            required: true,
            widget: "file_save",
            artifact: "video",
        }
        input "srt_output_path": "path" {
            description: "Destination SRT for the selected supplied language track.",
            required: true,
            widget: "file_save",
            artifact: "text",
        }
        input "transcription_request": "json" {
            description: "Reserved external-ASR request. The bundled runner has no ASR provider; supply its verified result as subtitle_tracks instead.",
            required: false,
            default: null,
            widget: "json",
        }
        input "translation_request": "json" {
            description: "Reserved external-translation request. The bundled runner has no translation provider; supply translated tracks explicitly instead.",
            required: false,
            default: null,
            widget: "json",
        }
        output "video": "artifact" {
            description: "MP4 with selected subtitle language burned in.",
            artifact: "video",
        }
        output "video_path": "path" {
            description: "Path to the MP4 with burned subtitles.",
            artifact: "video",
        }
        output "subtitles": "artifact" {
            description: "Selected-language SRT artifact metadata.",
            artifact: "text",
        }
        output "subtitles_path": "path" {
            description: "Path to the selected-language SRT file.",
            artifact: "text",
        }
        output "render_summary": "text" {
            description: "Human-readable subtitle export and burn-in summary.",
        }
    }
        .builtin_runtime("runner", "lightflow.runner", "runner.v1")
        .build()
}

pub fn execute(
    inputs: &Map<String, Value>,
) -> Result<Response, lightflow_auto_edit_runtime::RuntimeError> {
    lightflow_auto_edit_runtime::execute(
        WORKFLOW_ID,
        WORKFLOW_VERSION,
        inputs,
        include_str!("lib.rs"),
    )
}
