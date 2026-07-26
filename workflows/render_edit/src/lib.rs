use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_render_edit";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Video Render Edit",
        description: "Render a structured edit decision plan into a video artifact using the repository ffmpeg renderer.",
        input "edit_plan": "json" {
            description: "Edit decision plan with output settings and ordered timeline segments.",
            required: true,
            widget: "json",
        }
        input "output_path": "path" {
            description: "Destination MP4 path for the rendered edit.",
            required: true,
            widget: "file_save",
            artifact: "video",
        }
        output "video": "artifact" {
            description: "Rendered video artifact metadata.",
            artifact: "video",
        }
        output "video_path": "path" {
            description: "Path to the rendered MP4 file.",
            artifact: "video",
        }
        output "render_summary": "text" {
            description: "Human-readable render summary.",
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
