use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
        input "clips": "json" {
            description: "Array of source clip records with ids, paths, optional start/end times, transcript snippets, scores, tags, or media analysis.",
            required: true,
            widget: "json",
        }
        input "brief": "text" {
            description: "Human editing goal, story outline, or narration/script notes.",
            required: true,
            widget: "textarea",
        }
        input "style": "text" {
            description: "Editing style such as tutorial, vlog recap, product demo, shorts cut, or calm documentary.",
            required: false,
            default: "clean social edit",
            widget: "textarea",
        }
        input "constraints": "json" {
            description: "Delivery constraints such as aspect_ratio, max_duration_seconds, fps, caption language, music policy, or platform.",
            required: false,
            default: {},
            widget: "json",
        }
        output "edit_plan": "json" {
            description: "Serializable edit decision plan with selected segments, ordering, transitions, captions, audio notes, and render hints.",
        }
        output "summary": "text" {
            description: "Human-readable summary of the planned edit.",
        }
    }
        .name("Video Auto Edit Plan")
        .description("Plan an automated video edit from source clips, narrative goals, style guidance, and delivery constraints.")
        .build()
}
