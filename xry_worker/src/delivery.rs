use super::{ROOT, atomic_json, covers::validate_cover_spec, read_json};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

pub(super) fn package(task: &str, subject: &str, production: &Path) -> Result<Value, String> {
    package_at_root(task, subject, production, Path::new(ROOT))
}

fn package_at_root(
    task: &str,
    subject: &str,
    production: &Path,
    root: &Path,
) -> Result<Value, String> {
    let gate = read_json(&production.join("quality-gate.json"))?;
    if gate.get("status").and_then(Value::as_str) != Some("PASS") {
        return Err("package requires a passing quality gate".to_owned());
    }
    validate_cover_spec(production)?;
    let ids = read_json(&production.join("video-id.json"))?;
    let group = ids
        .get("group_name")
        .and_then(Value::as_str)
        .ok_or("video-id group_name is missing")?;
    let ze_id = ids
        .get("zh_en_variant_id")
        .and_then(Value::as_str)
        .ok_or("video-id ZE variant is missing")?;
    let re_id = ids
        .get("ru_en_variant_id")
        .and_then(Value::as_str)
        .ok_or("video-id RE variant is missing")?;
    let publication = read_json(&production.join("publication.json"))?;
    let ze_copy = publication
        .pointer("/ze/copy")
        .and_then(Value::as_str)
        .ok_or("publication.ze.copy is missing")?;
    let re_copy = publication
        .pointer("/re/copy")
        .and_then(Value::as_str)
        .ok_or("publication.re.copy is missing")?;
    let delivery = root.join("3.成品");
    let ze_dir = delivery.join(group);
    let re_dir = delivery.join("Ty Sun Motors");
    let ze_files = [
        (
            production.join("rendered.ze.mp4"),
            ze_dir.join(format!("{ze_id}.mp4")),
        ),
        (
            production.join("cover.ze.jpg"),
            ze_dir.join(format!("{ze_id}.jpg")),
        ),
    ];
    let re_files = [
        (
            production.join("rendered.re.mp4"),
            re_dir.join(format!("{re_id}.mp4")),
        ),
        (
            production.join("cover.re.jpg"),
            re_dir.join(format!("{re_id}.jpg")),
        ),
    ];
    for (source, destination) in ze_files.iter().chain(re_files.iter()) {
        atomic_copy(source, destination)?;
    }
    atomic_text(&ze_dir.join(format!("{ze_id}.txt")), ze_copy)?;
    atomic_text(&re_dir.join(format!("{re_id}.txt")), re_copy)?;
    let receipt = json!({"schema_version":1,"status":"PACKAGED","task":task,"subject":subject,"deliveries":{"ZE":{"account":group,"id":ze_id},"RE":{"account":"Ty Sun Motors","id":re_id}},"quality_gate":gate});
    atomic_json(&production.join("delivery-receipt.json"), &receipt)?;
    Ok(receipt)
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
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn package_revalidates_cover_spec_before_creating_delivery() {
        let root = temporary_directory("delivery-root");
        let production = temporary_directory("production");
        fs::write(production.join("quality-gate.json"), r#"{"status":"PASS"}"#)
            .expect("write passing gate");
        fs::write(
            production.join("cover-spec.json"),
            r#"{"headline_ru":"Новый пикап"}"#,
        )
        .expect("write invalid cover spec");

        let error = package_at_root("批量剪辑/test/7.23批量", "S01", &production, &root)
            .expect_err("stale passing gate must not package invalid cover text");
        assert_eq!(error, "cover headline_zh is required");
        assert!(!root.join("3.成品").exists());
        assert!(!production.join("delivery-receipt.json").exists());

        fs::remove_dir_all(root).expect("remove temporary delivery root");
        fs::remove_dir_all(production).expect("remove temporary production");
    }

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-xry-worker-delivery-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        path
    }
}
