use std::path::Path;

use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};

use crate::RuntimeError;
use crate::evidence::{
    EvidenceFields, VIDEOSCORE_MODEL, VIDEOSCORE_WORKFLOW, evidence_key, verify_evidence,
    verify_highlight_signature,
};
use crate::media::{PLAN_SCHEMA, input_text, number, probe, source_path, text};

#[cfg(test)]
use crate::evidence::evidence_message;

pub(crate) struct PlannedEdit {
    pub(crate) plan: Value,
    pub(crate) summary: String,
}

pub(crate) fn execute(inputs: &Map<String, Value>, base: &Path) -> Result<Response, RuntimeError> {
    let planned = build(inputs, base)?;
    Ok(Response {
        outputs: Map::from_iter([
            ("edit_plan".to_owned(), planned.plan),
            ("summary".to_owned(), planned.summary.into()),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::new(),
    })
}

pub(crate) fn build(inputs: &Map<String, Value>, base: &Path) -> Result<PlannedEdit, RuntimeError> {
    let clips = inputs
        .get("clips")
        .or_else(|| inputs.get("sources"))
        .and_then(Value::as_array)
        .filter(|clips| !clips.is_empty())
        .ok_or_else(|| RuntimeError::new("clips or sources must be a non-empty array"))?;
    let brief = input_text(inputs, "brief")?;
    let style = inputs
        .get("style")
        .map(|value| text(value, "style"))
        .transpose()?
        .unwrap_or("clean social edit");
    let constraints = inputs.get("constraints").and_then(Value::as_object);
    if constraints
        .and_then(|value| value.get("auto_segment"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(RuntimeError::new(
            "auto_segment needs an explicit analysis workflow; it is not silently approximated",
        ));
    }
    let output = output(constraints)?;
    for (index, raw) in clips.iter().enumerate() {
        highlight_object(clip_object(raw, index)?, index)?;
    }
    let evidence_key = evidence_key()?;
    let mut timeline = Vec::new();
    for (index, raw) in clips.iter().enumerate() {
        let object = clip_object(raw, index)?;
        verify_highlight_signature(object, index, &evidence_key)?;
        let path = source_path(
            base,
            text(
                object
                    .get("path")
                    .ok_or_else(|| RuntimeError::new(format!("clips[{index}].path missing")))?,
                &format!("clips[{index}].path"),
            )?,
            "clip path",
        )?;
        let (duration, has_audio) = probe(&path)?;
        let start = object
            .get("start")
            .map(|value| number(Some(value), "clip start"))
            .transpose()?
            .unwrap_or(0.0);
        let end = object
            .get("end")
            .map(|value| number(Some(value), "clip end"))
            .transpose()?
            .unwrap_or(duration);
        if !(0.0 <= start && start < end && end <= duration + 0.001) {
            return Err(RuntimeError::new(format!(
                "clips[{index}] has invalid start/end bounds"
            )));
        }
        let highlight = verified_highlight(object, index, base, &path, start, end, &evidence_key)?;
        let clip_id = object
            .get("id")
            .or_else(|| object.get("clip_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("clip-{}", index + 1));
        timeline.push(json!({
            "clip_id": clip_id,
            "path": crate::media::artifact_path(base, &path)?,
            "start": start,
            "end": end,
            "duration": end - start,
            "has_audio": has_audio,
            "highlight": highlight,
            "title": object.get("title").cloned().unwrap_or(Value::Null),
            "subtitle": object.get("subtitle").cloned().unwrap_or(Value::Null),
        }));
    }
    let maximum = output
        .get("max_duration_seconds")
        .and_then(Value::as_f64)
        .unwrap_or(60.0);
    timeline.truncate(choose_duration(&timeline, maximum));
    if timeline.is_empty() {
        return Err(RuntimeError::new("no clip fits the requested duration"));
    }
    let duration = timeline
        .iter()
        .filter_map(|value| value.get("duration").and_then(Value::as_f64))
        .sum::<f64>();
    let plan = json!({
        "schema": PLAN_SCHEMA,
        "brief": brief,
        "style": style,
        "timeline": timeline,
        "output": output,
        "provenance": {"planner": "lightflow.rust.media-metadata.v1", "duration_seconds": duration}
    });
    Ok(PlannedEdit {
        summary: format!(
            "Rust-native planner selected {} clip(s), {:.3}s total.",
            plan["timeline"].as_array().map_or(0, Vec::len),
            duration
        ),
        plan,
    })
}

fn clip_object(raw: &Value, index: usize) -> Result<&Map<String, Value>, RuntimeError> {
    raw.as_object().ok_or_else(|| {
        RuntimeError::new(format!(
            "clips[{index}] must be an object with VideoScore highlight provenance"
        ))
    })
}

fn verified_highlight(
    clip: &Map<String, Value>,
    index: usize,
    base: &Path,
    clip_path: &Path,
    start: f64,
    end: f64,
    key: &[u8],
) -> Result<Value, RuntimeError> {
    let highlight = highlight_object(clip, index)?;
    let highlight_start = number(
        highlight.get("start_seconds"),
        &format!("clips[{index}].highlight.start_seconds"),
    )?;
    let highlight_end = number(
        highlight.get("end_seconds"),
        &format!("clips[{index}].highlight.end_seconds"),
    )?;
    if (highlight_start - start).abs() > 0.001 || (highlight_end - end).abs() > 0.001 {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight time range must match the clip range within 0.001 seconds"
        )));
    }
    let score = number(
        highlight.get("score"),
        &format!("clips[{index}].highlight.score"),
    )?;
    if !(1.0..=4.0).contains(&score) {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.score must be between 1 and 4"
        )));
    }
    let model = text(
        highlight
            .get("model")
            .ok_or_else(|| RuntimeError::new(format!("clips[{index}].highlight.model missing")))?,
        &format!("clips[{index}].highlight.model"),
    )?;
    let reason = text(
        highlight
            .get("reason")
            .ok_or_else(|| RuntimeError::new(format!("clips[{index}].highlight.reason missing")))?,
        &format!("clips[{index}].highlight.reason"),
    )?;
    let workflow = text(
        highlight.get("workflow").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.workflow missing"))
        })?,
        &format!("clips[{index}].highlight.workflow"),
    )?;
    if workflow != VIDEOSCORE_WORKFLOW {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.workflow must be {VIDEOSCORE_WORKFLOW}"
        )));
    }
    if model != VIDEOSCORE_MODEL {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.model must be {VIDEOSCORE_MODEL}"
        )));
    }
    let evidence = text(
        highlight.get("evidence").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.evidence missing"))
        })?,
        &format!("clips[{index}].highlight.evidence"),
    )?;
    let fields = EvidenceFields {
        source_path: source_path_text(highlight, index)?,
        model,
        start: highlight_start,
        end: highlight_end,
        score,
        reason,
    };
    verify_evidence(key, &fields, evidence)?;
    let source = source_path(base, fields.source_path, "highlight source_path")?;
    if source != clip_path {
        return Err(RuntimeError::new(format!(
            "clips[{index}].highlight.source_path must match clips[{index}].path after canonicalization"
        )));
    }
    Ok(json!({
        "workflow": workflow,
        "source_path": crate::media::artifact_path(base, &source)?,
        "start_seconds": highlight_start,
        "end_seconds": highlight_end,
        "score": score,
        "model": model,
        "evidence": evidence,
        "reason": reason,
    }))
}

fn highlight_object(
    clip: &Map<String, Value>,
    index: usize,
) -> Result<&Map<String, Value>, RuntimeError> {
    clip.get("highlight")
        .and_then(Value::as_object)
        .ok_or_else(|| RuntimeError::new(format!("clips[{index}].highlight must be an object")))
}

fn source_path_text(highlight: &Map<String, Value>, index: usize) -> Result<&str, RuntimeError> {
    text(
        highlight.get("source_path").ok_or_else(|| {
            RuntimeError::new(format!("clips[{index}].highlight.source_path missing"))
        })?,
        &format!("clips[{index}].highlight.source_path"),
    )
}

fn choose_duration(timeline: &[Value], maximum: f64) -> usize {
    let mut total = 0.0;
    for (index, segment) in timeline.iter().enumerate() {
        total += segment
            .get("duration")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        if total > maximum + 0.001 {
            return index;
        }
    }
    timeline.len()
}

fn output(constraints: Option<&Map<String, Value>>) -> Result<Value, RuntimeError> {
    let defaults = match constraints
        .and_then(|value| value.get("aspect_ratio"))
        .and_then(Value::as_str)
        .unwrap_or("9:16")
    {
        "9:16" => (720, 1280),
        "16:9" => (1280, 720),
        "1:1" => (1080, 1080),
        "4:5" => (864, 1080),
        _ => {
            return Err(RuntimeError::new(
                "aspect_ratio must be 9:16, 16:9, 1:1, or 4:5",
            ));
        }
    };
    let width = constraints
        .and_then(|value| value.get("width"))
        .and_then(Value::as_u64)
        .unwrap_or(defaults.0) as u32;
    let height = constraints
        .and_then(|value| value.get("height"))
        .and_then(Value::as_u64)
        .unwrap_or(defaults.1) as u32;
    let fps = constraints
        .and_then(|value| value.get("fps"))
        .and_then(Value::as_u64)
        .unwrap_or(30) as u32;
    let maximum = constraints
        .and_then(|value| value.get("max_duration_seconds"))
        .and_then(Value::as_f64)
        .unwrap_or(60.0);
    if width < 16
        || height < 16
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || !(1..=120).contains(&fps)
        || !(0.25..=21600.0).contains(&maximum)
    {
        return Err(RuntimeError::new("output constraints are invalid"));
    }
    Ok(json!({"width": width, "height": height, "fps": fps, "max_duration_seconds": maximum}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::hmac;
    use std::fs;
    #[test]
    fn duration_cut_keeps_whole_segments() {
        let timeline = vec![json!({"duration": 2.0}), json!({"duration": 3.0})];
        assert_eq!(choose_duration(&timeline, 4.0), 1);
    }

    #[test]
    fn verified_highlight_requires_matching_canonical_source_and_range() {
        let directory =
            std::env::temp_dir().join(format!("lightflow-auto-edit-plan-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create test directory");
        let source = directory.join("source.mp4");
        fs::write(&source, []).expect("create source file");
        let clip = json!({
            "highlight": {
                "source_path": "./source.mp4",
                "start_seconds": 1.0,
                "end_seconds": 4.0,
                "score": 3.4,
                "model": "TIGER-Lab/VideoScore-v1.1",
                "workflow": "lightflow.video_highlights",
                "reason": "Vehicle exterior is clearly visible."
            }
        });
        let key = b"0123456789abcdef0123456789abcdef";
        let mut clip = clip;
        clip["highlight"]["evidence"] = signed_evidence(
            key,
            "./source.mp4",
            VIDEOSCORE_MODEL,
            1.0,
            4.0,
            3.4,
            "Vehicle exterior is clearly visible.",
        )
        .into();
        let object = clip.as_object().expect("clip object");
        let canonical_source = source.canonicalize().expect("canonical source");
        let provenance =
            verified_highlight(object, 0, &directory, &canonical_source, 1.0, 4.0, key)
                .expect("matching VideoScore evidence");
        assert_eq!(provenance["source_path"], "source.mp4");

        let mut mismatch = object.clone();
        mismatch["highlight"]["end_seconds"] = 4.01.into();
        assert!(
            verified_highlight(&mismatch, 0, &directory, &canonical_source, 1.0, 4.0, key,)
                .is_err()
        );

        fs::remove_file(source).expect("remove source file");
        fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn verified_highlight_rejects_missing_required_evidence() {
        let directory = std::env::temp_dir().join(format!(
            "lightflow-auto-edit-plan-missing-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create test directory");
        let source = directory.join("source.mp4");
        fs::write(&source, []).expect("create source file");
        let clip = json!({
            "highlight": {
                "source_path": "source.mp4",
                "start_seconds": 0.0,
                "end_seconds": 2.0,
                "score": 4.1,
                "model": "",
                "workflow": "lightflow.video_highlights",
                "evidence": "0000000000000000000000000000000000000000000000000000000000000000",
                "reason": ""
            }
        });
        let canonical_source = source.canonicalize().expect("canonical source");
        assert!(
            verified_highlight(
                clip.as_object().expect("clip object"),
                0,
                &directory,
                &canonical_source,
                0.0,
                2.0,
                b"0123456789abcdef0123456789abcdef",
            )
            .is_err()
        );

        fs::remove_file(source).expect("remove source file");
        fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn clip_object_rejects_string_only_source() {
        assert!(clip_object(&json!("media/source.mp4"), 0).is_err());
    }

    #[test]
    fn evidence_round_trips_and_rejects_tampering() {
        let key = b"0123456789abcdef0123456789abcdef";
        let evidence = signed_evidence(
            key,
            "media/source.mp4",
            VIDEOSCORE_MODEL,
            1.0,
            4.0,
            3.4,
            "Clear vehicle shot.",
        );
        let fields = EvidenceFields {
            source_path: "media/source.mp4",
            model: VIDEOSCORE_MODEL,
            start: 1.0,
            end: 4.0,
            score: 3.4,
            reason: "Clear vehicle shot.",
        };
        assert!(verify_evidence(key, &fields, &evidence).is_ok());
        let tampered = EvidenceFields {
            reason: "Different reason.",
            ..fields
        };
        assert!(verify_evidence(key, &tampered, &evidence).is_err());
    }

    fn signed_evidence(
        key: &[u8],
        source_path: &str,
        model: &str,
        start: f64,
        end: f64,
        score: f64,
        reason: &str,
    ) -> String {
        let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key);
        let tag = hmac::sign(
            &signing_key,
            &evidence_message(&EvidenceFields {
                source_path,
                model,
                start,
                end,
                score,
                reason,
            }),
        );
        tag.as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
