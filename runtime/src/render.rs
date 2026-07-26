use std::path::Path;
use std::process::Command;

use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};

use crate::RuntimeError;
use crate::media::{
    PLAN_SCHEMA, artifact_path, command, input_text, number, output_path, probe, source_path,
};

pub(crate) fn execute(inputs: &Map<String, Value>, base: &Path) -> Result<Response, RuntimeError> {
    let plan = inputs
        .get("edit_plan")
        .ok_or_else(|| RuntimeError::new("missing required input edit_plan"))?;
    execute_plan(plan, "", inputs, base)
}

pub(crate) fn execute_plan(
    plan: &Value,
    planner_summary: &str,
    inputs: &Map<String, Value>,
    base: &Path,
) -> Result<Response, RuntimeError> {
    let object = plan
        .as_object()
        .ok_or_else(|| RuntimeError::new("edit_plan must be an object"))?;
    if object.get("schema").and_then(Value::as_str) != Some(PLAN_SCHEMA) {
        return Err(RuntimeError::new("edit_plan has an unsupported schema"));
    }
    let output = output_path(base, input_text(inputs, "output_path")?, "output_path")?;
    let settings = object
        .get("output")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::new("edit_plan.output must be an object"))?;
    let width = settings
        .get("width")
        .and_then(Value::as_u64)
        .ok_or_else(|| RuntimeError::new("edit_plan.output.width missing"))?;
    let height = settings
        .get("height")
        .and_then(Value::as_u64)
        .ok_or_else(|| RuntimeError::new("edit_plan.output.height missing"))?;
    let fps = settings
        .get("fps")
        .and_then(Value::as_u64)
        .ok_or_else(|| RuntimeError::new("edit_plan.output.fps missing"))?;
    let timeline = object
        .get("timeline")
        .and_then(Value::as_array)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RuntimeError::new("edit_plan.timeline must be non-empty"))?;
    let mut command_line = Command::new("ffmpeg");
    command_line.args(["-y", "-hide_banner", "-nostdin", "-loglevel", "error"]);
    let mut filters = Vec::new();
    let mut labels = Vec::new();
    let mut duration = 0.0;
    for (index, segment) in timeline.iter().enumerate() {
        let item = segment
            .as_object()
            .ok_or_else(|| RuntimeError::new("timeline segment must be an object"))?;
        let path = source_path(
            base,
            item.get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RuntimeError::new("timeline segment path missing"))?,
            "timeline path",
        )?;
        let (source_duration, has_audio) = probe(&path)?;
        if !has_audio {
            return Err(RuntimeError::new(
                "render requires audio on every timeline source",
            ));
        }
        let start = number(item.get("start"), "timeline start")?;
        let end = number(item.get("end"), "timeline end")?;
        if !(0.0 <= start && start < end && end <= source_duration + 0.001) {
            return Err(RuntimeError::new("timeline bounds are invalid"));
        }
        duration += end - start;
        command_line.args([
            "-ss",
            &format!("{start:.6}"),
            "-t",
            &format!("{:.6}", end - start),
            "-i",
        ]);
        command_line.arg(path);
        filters.push(format!("[{index}:v]scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},format=yuv420p[v{index}]"));
        filters.push(format!(
            "[{index}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{index}]"
        ));
        labels.push(format!("[v{index}][a{index}]"));
    }
    filters.push(format!(
        "{}concat=n={}:v=1:a=1[outv][outa]",
        labels.join(""),
        timeline.len()
    ));
    command_line.args([
        "-filter_complex",
        &filters.join(";"),
        "-map",
        "[outv]",
        "-map",
        "[outa]",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "23",
        "-c:a",
        "aac",
        "-b:a",
        "160k",
        "-movflags",
        "+faststart",
    ]);
    command_line.arg(&output);
    command(&mut command_line, "FFmpeg render")?;
    let artifact = json!({"id":"video", "kind":"video", "path":artifact_path(base, &output)?, "mime_type":"video/mp4", "metadata":{"duration_seconds":duration,"segments":timeline.len(),"width":width,"height":height,"fps":fps,"bytes":std::fs::metadata(&output).map_err(RuntimeError::io)?.len()}});
    let planner_prefix = if planner_summary.is_empty() {
        String::new()
    } else {
        format!("{planner_summary} ")
    };
    let summary = format!(
        "{planner_prefix}Rendered {} Rust-native timeline segments into {}.",
        timeline.len(),
        output.display()
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("video".to_owned(), artifact),
            (
                "video_path".to_owned(),
                artifact_path(base, &output)?.into(),
            ),
            ("render_summary".to_owned(), summary.clone().into()),
            ("summary".to_owned(), summary.into()),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::new(),
    })
}
