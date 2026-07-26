use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use super::{covers::validate_cover_spec, field_number, is_cjk, is_cyrillic, read_json};

pub(super) fn audit_output(
    production: &Path,
    edl: &Value,
    ze_ass: &Path,
    re_ass: &Path,
) -> Result<Value, String> {
    let expected_duration = timeline_duration(edl)?;
    let ze_video = production.join("rendered.ze.mp4");
    let re_video = production.join("rendered.re.mp4");
    let ze_duration = video_duration(&ze_video)?;
    let re_duration = video_duration(&re_video)?;
    if (ze_duration - expected_duration).abs() > 0.2
        || (re_duration - expected_duration).abs() > 0.2
    {
        return Err("rendered video duration does not match frozen EDL".to_owned());
    }
    let ze_cover =
        fs::metadata(production.join("cover.ze.jpg")).map_err(|error| error.to_string())?;
    let re_cover =
        fs::metadata(production.join("cover.re.jpg")).map_err(|error| error.to_string())?;
    if ze_cover.ino() == re_cover.ino() || ze_cover.len() == 0 || re_cover.len() == 0 {
        return Err("account covers must be separate non-empty files".to_owned());
    }
    let cover_profile = validate_cover_spec(production)?;
    let ze_text = fs::read_to_string(ze_ass).map_err(|error| error.to_string())?;
    let re_text = fs::read_to_string(re_ass).map_err(|error| error.to_string())?;
    if !ze_text.chars().any(is_cjk) || !re_text.chars().any(is_cyrillic) {
        return Err("subtitle tracks do not match ZE/RE language contract".to_owned());
    }
    let publication = read_json(&production.join("publication.json"))?;
    let ze_copy = publication
        .pointer("/ze/copy")
        .and_then(Value::as_str)
        .ok_or("publication.ze.copy is missing")?;
    let re_copy = publication
        .pointer("/re/copy")
        .and_then(Value::as_str)
        .ok_or("publication.re.copy is missing")?;
    if !ze_copy.chars().any(is_cjk) || !re_copy.chars().any(is_cyrillic) {
        return Err("publication copy does not match ZE/RE account language contract".to_owned());
    }
    Ok(
        json!({"duration_seconds":{"expected":expected_duration,"ze":ze_duration,"re":re_duration},"covers":{"profile_id":cover_profile,"ze_inode":ze_cover.ino(),"re_inode":re_cover.ino(),"separate":true},"subtitles":{"ze_has_cjk":true,"re_has_cyrillic":true},"publication":{"ze_has_cjk":true,"re_has_cyrillic":true}}),
    )
}

pub(super) fn timeline_duration(edl: &Value) -> Result<f64, String> {
    if let Some(duration) = edl
        .get("timeline_duration_seconds")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        return Ok(duration);
    }
    let segments = edl
        .get("video_segments")
        .and_then(Value::as_array)
        .filter(|segments| !segments.is_empty())
        .ok_or("edl timeline duration is missing and video_segments are unavailable")?;
    let mut expected_in = 0.0;
    for segment in segments {
        let item = segment
            .as_object()
            .ok_or("edl segment must be an object when deriving duration")?;
        let timeline_in = field_number(item, "timeline_in")?;
        let timeline_out = field_number(item, "timeline_out")?;
        if !(timeline_in >= 0.0 && timeline_out > timeline_in)
            || (timeline_in - expected_in).abs() > 0.001
        {
            return Err("edl timeline segments are discontinuous".to_owned());
        }
        expected_in = timeline_out;
    }
    Ok(expected_in)
}

fn video_duration(path: &Path) -> Result<f64, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!("cannot probe {}", path.display()));
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|error| error.to_string())?
        .pointer("/format/duration")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| format!("invalid duration for {}", path.display()))
}
