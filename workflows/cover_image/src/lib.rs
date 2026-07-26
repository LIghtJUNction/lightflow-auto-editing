use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_cover_image";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Video Cover Image",
        description: "Compose an account-specific, non-black cover from a source-video frame and title.",
        input "source_path": "path" {
            description: "Input video path. Relative paths resolve from the LightFlow project root.",
            required: true,
            widget: "file_open",
        }
        input "timestamp_seconds": "number" {
            description: "Source-video timestamp for the cover frame.",
            required: true,
        }
        input "account_group": "text" {
            description: "zh uses the warm Chinese-account cover system; overseas uses the blue-cyan overseas-account system.",
            required: true,
            widget: "select",
            choices: ["zh", "overseas"],
        }
        input "output_path": "path" {
            description: "Destination PNG, JPG, or JPEG cover image.",
            required: true,
            widget: "file_save",
            artifact: "image",
        }
        input "title": "text" {
            description: "Required UTF-8 title burned onto the source frame using the account-specific style.",
            required: true,
            widget: "textarea",
        }
        input "font_path": "path" {
            description: "Explicit font file for title rendering; choose a font with needed CJK/Cyrillic glyphs.",
            required: true,
            widget: "file_open",
        }
        output "cover": "artifact" {
            description: "Generated PNG or JPEG cover artifact metadata.",
            artifact: "image",
        }
        output "cover_path": "path" {
            description: "Path to the generated cover image.",
            artifact: "image",
        }
        output "summary": "text" {
            description: "Human-readable cover extraction summary.",
        }
    }
        .builtin_runtime(
            "command",
            "lightflow.command.run",
            "runner.v1",
        )
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
