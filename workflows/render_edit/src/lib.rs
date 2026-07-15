use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
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
        .name("Video Render Edit")
        .description("Render a structured edit decision plan into a video artifact using the repository ffmpeg renderer.")
        .build()
}
