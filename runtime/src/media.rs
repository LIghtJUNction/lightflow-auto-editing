use std::path::{Path, PathBuf};
use std::process::Command;

use lightflow::serde_json::Value;

use crate::RuntimeError;

pub(crate) const PLAN_SCHEMA: &str = "lightflow.video.edit-plan.v1";

pub(crate) fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, RuntimeError> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RuntimeError::new(format!("{field} must be non-empty text")))
}

pub(crate) fn input_text<'a>(
    inputs: &'a lightflow::serde_json::Map<String, Value>,
    name: &str,
) -> Result<&'a str, RuntimeError> {
    inputs
        .get(name)
        .ok_or_else(|| RuntimeError::new(format!("missing required input {name}")))
        .and_then(|value| text(value, name))
}

pub(crate) fn project_path(base: &Path, value: &str, field: &str) -> Result<PathBuf, RuntimeError> {
    let path = Path::new(value);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let resolved = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
    if !resolved.starts_with(base) {
        return Err(RuntimeError::new(format!(
            "{field} must resolve beneath the project root"
        )));
    }
    Ok(resolved)
}

pub(crate) fn source_path(base: &Path, value: &str, field: &str) -> Result<PathBuf, RuntimeError> {
    let path = project_path(base, value, field)?;
    if !path.is_file() {
        return Err(RuntimeError::new(format!(
            "{field} does not exist or is not a file: {}",
            path.display()
        )));
    }
    Ok(path)
}

pub(crate) fn output_path(base: &Path, value: &str, field: &str) -> Result<PathBuf, RuntimeError> {
    let path = project_path(base, value, field)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(RuntimeError::io)?;
    }
    Ok(path)
}

pub(crate) fn number(value: Option<&Value>, field: &str) -> Result<f64, RuntimeError> {
    let number = value
        .and_then(Value::as_f64)
        .ok_or_else(|| RuntimeError::new(format!("{field} must be a number")))?;
    if !number.is_finite() {
        return Err(RuntimeError::new(format!("{field} must be finite")));
    }
    Ok(number)
}

pub(crate) fn command(command: &mut Command, label: &str) -> Result<String, RuntimeError> {
    let output = command.output().map_err(RuntimeError::io)?;
    if !output.status.success() {
        return Err(RuntimeError::new(format!(
            "{label} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| RuntimeError::new(format!("{label} returned non-UTF-8 output")))
}

pub(crate) fn probe(path: &Path) -> Result<(f64, bool), RuntimeError> {
    let output = command(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration:stream=codec_type",
                "-of",
                "json",
            ])
            .arg(path),
        "ffprobe",
    )?;
    let payload: Value = lightflow::serde_json::from_str(&output)
        .map_err(|error| RuntimeError::new(format!("ffprobe returned invalid JSON: {error}")))?;
    let duration = payload
        .pointer("/format/duration")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| RuntimeError::new("ffprobe returned an invalid duration"))?;
    let has_video = payload
        .get("streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        });
    let has_audio = payload
        .get("streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| {
            streams
                .iter()
                .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"))
        });
    if !has_video {
        return Err(RuntimeError::new("source has no video stream"));
    }
    Ok((duration, has_audio))
}

pub(crate) fn artifact_path(base: &Path, path: &Path) -> Result<String, RuntimeError> {
    path.strip_prefix(base)
        .map(|value| value.to_string_lossy().into_owned())
        .map_err(|_| RuntimeError::new("artifact is outside project root"))
}
