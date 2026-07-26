#[cfg(test)]
use super::covers::{
    materialize_cover_original_with_reference_root, validate_cover_spec_with_reference_root,
};
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
    let title = ids
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && !value.contains('/'))
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
    let cover_original = production.join("cover-original.png");
    if !cover_original.is_file() {
        return Err("cover-original.png is missing in production".to_owned());
    }
    let delivery = root.join("3.成品");
    let mut receipts = Vec::new();
    for (account, id, video, cover) in [
        (group, ze_id, "rendered.ze.mp4", "cover.ze.jpg"),
        ("Ty Sun Motors", re_id, "rendered.re.mp4", "cover.re.jpg"),
    ] {
        let basename = format!("{subject_number}.{id}：{title}");
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
        let copy = if id.ends_with("-ZE") {
            ze_copy
        } else {
            re_copy
        };
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
    use std::path::PathBuf;
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

    #[test]
    fn canonical_directory_delivery_with_four_files() {
        let root = temporary_directory("delivery-canonical-root");
        let production = temporary_directory("production-canonical");
        let reference_root = temporary_directory("cover-references");
        let group = "专搞皮卡";
        let title = "示例皮卡";
        let ze_id = "S01-ZE";
        let re_id = "S01-RE";
        let reference = reference_root.join(group).join("reference.PNG");
        let reference_bytes = b"\x89PNG\r\n\x1a\nreference fixture";
        fs::create_dir_all(reference.parent().expect("reference parent"))
            .expect("create reference directory");
        fs::write(&reference, reference_bytes).expect("write reference image");
        fs::write(production.join("quality-gate.json"), r#"{"status":"PASS"}"#)
            .expect("write passing gate");
        fs::write(
            production.join("cover-spec.json"),
            format!(
                r#"{{"group_name":"{group}","profile_id":"rounded-smoke-gold","headline_zh":"皮卡","headline_ru":"Пикап","style_references":[{{"path":"{}"}}]}}"#,
                reference.display()
            ),
        )
        .expect("write cover spec");
        fs::write(
            production.join("video-id.json"),
            format!(
                r#"{{"group_name":"{group}","zh_en_variant_id":"{ze_id}","ru_en_variant_id":"{re_id}","title":"{title}"}}"#
            ),
        )
        .expect("write video ids");
        fs::write(
            production.join("publication.json"),
            r#"{"ze":{"copy":"ZE publication copy"},"re":{"copy":"RE publication copy"}}"#,
        )
        .expect("write publication copy");
        for (name, contents) in [
            ("rendered.ze.mp4", b"ze video".as_slice()),
            ("cover.ze.jpg", b"ze cover".as_slice()),
            ("rendered.re.mp4", b"re video".as_slice()),
            ("cover.re.jpg", b"re cover".as_slice()),
        ] {
            fs::write(production.join(name), contents).expect("write package source");
        }
        materialize_cover_original_with_reference_root(&production, &reference_root)
            .expect("materialize original cover from temporary reference root");
        assert_eq!(
            fs::read(production.join("cover-original.png")).expect("read materialized original"),
            reference_bytes
        );

        let receipt = package_at_root_with_reference_root(
            "批量剪辑/test/7.23批量",
            "S01",
            &production,
            &root,
            &reference_root,
        )
        .expect("package canonical delivery");

        let ze_basename = format!("01.{ze_id}：{title}");
        let re_basename = format!("01.{re_id}：{title}");
        let ze_directory = root.join("3.成品").join(group).join(&ze_basename);
        let re_directory = root.join("3.成品").join("Ty Sun Motors").join(&re_basename);
        assert_exact_delivery_files(
            &ze_directory,
            &ze_basename,
            b"ze video",
            b"ze cover",
            reference_bytes,
            "ZE publication copy",
        );
        assert_exact_delivery_files(
            &re_directory,
            &re_basename,
            b"re video",
            b"re cover",
            reference_bytes,
            "RE publication copy",
        );

        assert_eq!(receipt["schema_version"], 2);
        assert_eq!(receipt["status"], "PACKAGED");
        assert_eq!(receipt["task"], "批量剪辑/test/7.23批量");
        assert_eq!(receipt["subject"], "S01");
        assert_eq!(receipt["basename_title"], title);
        assert_eq!(
            receipt
                .pointer("/deliveries/ZE/account")
                .and_then(Value::as_str),
            Some(group)
        );
        assert_eq!(
            receipt.pointer("/deliveries/ZE/id").and_then(Value::as_str),
            Some(ze_id)
        );
        assert_eq!(
            receipt
                .pointer("/deliveries/ZE/directory")
                .and_then(Value::as_str),
            ze_directory.to_str()
        );
        assert_eq!(
            receipt
                .pointer("/deliveries/RE/account")
                .and_then(Value::as_str),
            Some("Ty Sun Motors")
        );
        assert_eq!(
            receipt.pointer("/deliveries/RE/id").and_then(Value::as_str),
            Some(re_id)
        );
        assert_eq!(
            receipt
                .pointer("/deliveries/RE/directory")
                .and_then(Value::as_str),
            re_directory.to_str()
        );
        assert_eq!(
            receipt
                .pointer("/quality_gate/status")
                .and_then(Value::as_str),
            Some("PASS")
        );
        assert_eq!(
            read_json(&production.join("delivery-receipt.json")).expect("read receipt"),
            receipt
        );

        fs::remove_dir_all(root).expect("remove temporary delivery root");
        fs::remove_dir_all(production).expect("remove temporary production");
        fs::remove_dir_all(reference_root).expect("remove temporary reference root");
    }

    fn assert_exact_delivery_files(
        directory: &Path,
        basename: &str,
        expected_video: &[u8],
        expected_cover: &[u8],
        expected_original: &[u8],
        expected_copy: &str,
    ) {
        let mut files = fs::read_dir(directory)
            .expect("read canonical delivery directory")
            .map(|entry| {
                entry
                    .expect("read delivery entry")
                    .file_name()
                    .into_string()
                    .expect("delivery filenames are UTF-8")
            })
            .collect::<Vec<_>>();
        files.sort();
        let mut expected = vec![
            format!("{basename}.mp4"),
            format!("{basename}.jpg"),
            format!("{basename}-封面原图.png"),
            format!("{basename}.txt"),
        ];
        expected.sort();
        assert_eq!(files, expected);
        assert_eq!(
            fs::read(directory.join(format!("{basename}.mp4"))).expect("read video"),
            expected_video
        );
        assert_eq!(
            fs::read(directory.join(format!("{basename}.jpg"))).expect("read cover"),
            expected_cover
        );
        assert_eq!(
            fs::read(directory.join(format!("{basename}-封面原图.png")))
                .expect("read original cover"),
            expected_original
        );
        assert_eq!(
            fs::read_to_string(directory.join(format!("{basename}.txt")))
                .expect("read publication copy"),
            expected_copy
        );
    }

    fn temporary_directory(label: &str) -> PathBuf {
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
