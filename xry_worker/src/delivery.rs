#[cfg(test)]
use super::covers::{
    materialize_cover_original_with_reference_root, validate_cover_spec_with_reference_root,
};
use super::{ROOT, atomic_json, covers::validate_cover_spec, read_json};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

const MAX_FILENAME_COMPONENT_BYTES: usize = 255;

pub(super) fn package(task: &str, subject: &str, production: &Path) -> Result<Value, String> {
    package_at_root(task, subject, production, Path::new(ROOT))
}

fn package_at_root(
    task: &str,
    subject: &str,
    production: &Path,
    root: &Path,
) -> Result<Value, String> {
    package_at_root_with_cover_validation(task, subject, production, root, || {
        validate_cover_spec(production)
    })
}

#[cfg(test)]
fn package_at_root_with_reference_root(
    task: &str,
    subject: &str,
    production: &Path,
    root: &Path,
    reference_root: &Path,
) -> Result<Value, String> {
    package_at_root_with_cover_validation(task, subject, production, root, || {
        validate_cover_spec_with_reference_root(production, reference_root)
    })
}

fn package_at_root_with_cover_validation<F>(
    task: &str,
    subject: &str,
    production: &Path,
    root: &Path,
    validate_cover: F,
) -> Result<Value, String>
where
    F: FnOnce() -> Result<&'static str, String>,
{
    let gate = read_json(&production.join("quality-gate.json"))?;
    if gate.get("status").and_then(Value::as_str) != Some("PASS") {
        return Err("package requires a passing quality gate".to_owned());
    }
    validate_cover()?;
    let cover = read_json(&production.join("cover-spec.json"))?;
    let ids = read_json(&production.join("video-id.json"))?;
    let group = ids
        .get("group_name")
        .and_then(Value::as_str)
        .ok_or("video-id group_name is missing")?;
    let cover_group = cover
        .get("group_name")
        .and_then(Value::as_str)
        .ok_or("cover group_name is missing")?;
    if cover_group != group {
        return Err("cover and video-id group_name must match".to_owned());
    }
    let ze_id = ids
        .get("zh_en_variant_id")
        .and_then(Value::as_str)
        .ok_or("video-id ZE variant is missing")?;
    let re_id = ids
        .get("ru_en_variant_id")
        .and_then(Value::as_str)
        .ok_or("video-id RE variant is missing")?;
    let title = ids
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && !value.contains(['/', '\0']))
        .ok_or("video-id title is missing or unsafe")?;
    let subject_number = subject
        .strip_prefix('S')
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .ok_or("subject must be an S-number")?;
    let publication = read_json(&production.join("publication.json"))?;
    let ze_copy = publication
        .pointer("/ze/copy")
        .and_then(Value::as_str)
        .ok_or("publication.ze.copy is missing")?;
    let re_copy = publication
        .pointer("/re/copy")
        .and_then(Value::as_str)
        .ok_or("publication.re.copy is missing")?;
    let ze_basename = delivery_basename(subject_number, ze_id, title)?;
    let re_basename = delivery_basename(subject_number, re_id, title)?;
    let cover_original = production.join("cover-original.png");
    if !cover_original.is_file() {
        return Err("cover-original.png is missing in production".to_owned());
    }
    validate_delivery_sources(production)?;
    let delivery = root.join("3.成品");
    let mut receipts = Vec::new();
    for (account, id, video, cover, copy, basename) in [
        (
            group,
            ze_id,
            "rendered.ze.mp4",
            "cover.ze.jpg",
            ze_copy,
            ze_basename,
        ),
        (
            "Ty Sun Motors",
            re_id,
            "rendered.re.mp4",
            "cover.re.jpg",
            re_copy,
            re_basename,
        ),
    ] {
        let directory = delivery.join(account).join(&basename);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        atomic_copy(
            &production.join(video),
            &directory.join(format!("{basename}.mp4")),
        )?;
        atomic_copy(
            &production.join(cover),
            &directory.join(format!("{basename}.jpg")),
        )?;
        atomic_copy(
            &cover_original,
            &directory.join(format!("{basename}-封面原图.png")),
        )?;
        atomic_text(&directory.join(format!("{basename}.txt")), copy)?;
        receipts.push(json!({"account": account, "id": id, "directory": directory}));
    }
    let receipt = json!({
        "schema_version": 2,
        "status": "PACKAGED",
        "task": task,
        "subject": subject,
        "basename_title": title,
        "deliveries": {"ZE": receipts[0], "RE": receipts[1]},
        "quality_gate": gate,
    });
    atomic_json(&production.join("delivery-receipt.json"), &receipt)?;
    Ok(receipt)
}

fn validate_delivery_sources(production: &Path) -> Result<(), String> {
    for name in [
        "rendered.ze.mp4",
        "cover.ze.jpg",
        "rendered.re.mp4",
        "cover.re.jpg",
    ] {
        let source = production.join(name);
        if !source.is_file() {
            return Err(format!("package source is missing: {}", source.display()));
        }
    }
    Ok(())
}

fn delivery_basename(subject_number: &str, id: &str, title: &str) -> Result<String, String> {
    if id.trim().is_empty() || id.contains(['/', '\0']) {
        return Err("video-id variant is missing or unsafe".to_owned());
    }
    let basename = format!("{subject_number}.{id}：{title}");
    if ["", ".mp4", ".jpg", "-封面原图.png", ".txt"]
        .iter()
        .any(|suffix| basename.len() + suffix.len() > MAX_FILENAME_COMPONENT_BYTES)
    {
        return Err("delivery filename is too long".to_owned());
    }
    Ok(basename)
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("package source is missing: {}", source.display()));
    }
    let parent = destination
        .parent()
        .ok_or("delivery destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staged = parent.join(format!(
        ".{}.lightflow-staged",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("delivery destination has no valid filename")?
    ));
    fs::copy(source, &staged).map_err(|error| error.to_string())?;
    fs::rename(staged, destination).map_err(|error| error.to_string())
}

fn atomic_text(path: &Path, value: &str) -> Result<(), String> {
    let parent = path.parent().ok_or("text destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staged = parent.join(format!(
        ".{}.lightflow-staged",
        path.file_name()
            .and_then(|value| value.to_str())
            .ok_or("text destination has no valid filename")?
    ));
    fs::write(&staged, value).map_err(|error| error.to_string())?;
    fs::rename(staged, path).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
