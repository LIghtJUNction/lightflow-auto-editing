use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.video_auto_edit_plan";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Video Auto Edit Plan",
        description: "Plan a video edit from explicit VideoScore-verified source clip ranges, narrative goals, style guidance, and delivery constraints.",
        input "clips": "json" {
            description: "Array of clip objects. Every clip requires path, optional explicit start/end range, and an HMAC-verified VideoScore highlight object with workflow, source_path, start_seconds, end_seconds, score, model, reason, and evidence.",
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
            description: "Delivery constraints such as aspect_ratio, max_duration_seconds, fps, width, and height. The workflow never creates unverified source segments.",
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
        .builtin_runtime(
            "command",
            "lightflow.command.run",
            "process.command.v1",
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

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::json;
    use lightflow_video_highlights::sign_videoscore_evidence;
    use std::fs;
    use std::process::Command;
    use std::sync::{Mutex, OnceLock};

    const EVIDENCE_KEY: &str = "0123456789abcdef0123456789abcdef";
    const MODEL: &str = "TIGER-Lab/VideoScore-v1.1";
    const REASON: &str = "Clear vehicle exterior.";

    static EVIDENCE_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct TestDirectory(std::path::PathBuf);

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn execute_rejects_clip_without_highlight() {
        let inputs = Map::from_iter([
            (
                "clips".to_owned(),
                json!([{"path":"media/missing.mp4","start":0,"end":1}]),
            ),
            ("brief".to_owned(), json!("Select the vehicle highlight.")),
        ]);
        assert!(execute(&inputs).is_err());
    }

    #[test]
    fn execute_rejects_tampered_evidence_before_media_probe() {
        with_evidence_key(|| {
            let directory = test_directory("tampered");
            let source = directory.0.join("missing.mp4");
            let inputs = plan_inputs(&source, "0".repeat(64));
            let error = execute(&inputs).expect_err("tampered evidence must fail");
            assert!(error.to_string().contains("HMAC verification failed"));
        });
    }

    #[test]
    fn execute_accepts_signed_audiovisual_candidate() {
        with_evidence_key(|| {
            let directory = test_directory("valid");
            let source = create_audiovisual_fixture(&directory.0);
            let source_text = source.to_string_lossy().into_owned();
            let evidence = sign_videoscore_evidence(
                EVIDENCE_KEY.as_bytes(),
                &source_text,
                0.0,
                1.0,
                3.4,
                REASON,
            );
            let response = execute(&plan_inputs(&source, evidence.clone())).expect("signed plan");
            let timeline = response.outputs["edit_plan"]["timeline"]
                .as_array()
                .expect("timeline");
            assert_eq!(timeline.len(), 1);
            assert_eq!(timeline[0]["highlight"]["evidence"], evidence);
            assert_eq!(timeline[0]["highlight"]["model"], MODEL);
        });
    }

    fn plan_inputs(source: &std::path::Path, evidence: String) -> Map<String, Value> {
        let source = source.to_string_lossy();
        Map::from_iter([
            (
                "clips".to_owned(),
                json!([{"path":source,"start":0.0,"end":1.0,"highlight":{"workflow":"lightflow.video_highlights","source_path":source,"start_seconds":0.0,"end_seconds":1.0,"score":3.4,"model":MODEL,"reason":REASON,"evidence":evidence}}]),
            ),
            ("brief".to_owned(), json!("Select the vehicle highlight.")),
        ])
    }

    fn create_audiovisual_fixture(directory: &std::path::Path) -> std::path::PathBuf {
        let source = directory.join("fixture.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=64x64:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ])
            .arg(&source)
            .status()
            .expect("start ffmpeg fixture");
        assert!(status.success(), "ffmpeg fixture status: {status}");
        source
    }

    fn test_directory(label: &str) -> TestDirectory {
        let directory = std::env::current_dir()
            .expect("project directory")
            .join(format!(
                ".lightflow-test-auto-edit-plan-{label}-{}",
                std::process::id()
            ));
        fs::create_dir_all(&directory).expect("create test directory");
        TestDirectory(directory)
    }

    fn with_evidence_key<T>(action: impl FnOnce() -> T) -> T {
        let _guard = EVIDENCE_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY").ok();
        unsafe { std::env::set_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY", EVIDENCE_KEY) };
        let result = action();
        if let Some(previous) = previous {
            unsafe { std::env::set_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY", previous) };
        } else {
            unsafe { std::env::remove_var("LIGHTFLOW_VIDEOSCORE_EVIDENCE_KEY") };
        }
        result
    }
}
