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
fn package_preflights_all_sources_before_creating_delivery() {
    let root = temporary_directory("delivery-preflight-root");
    let production = temporary_directory("delivery-preflight-production");
    write_package_metadata(&production, "account", "account", "示例", "ze-id", "re-id");
    for name in [
        "cover-original.png",
        "rendered.ze.mp4",
        "cover.ze.jpg",
        "rendered.re.mp4",
    ] {
        fs::write(production.join(name), b"fixture").expect("write package source");
    }

    let error = package_at_root_with_cover_validation(
        "批量剪辑/test/7.23批量",
        "S01",
        &production,
        &root,
        || Ok("rounded-smoke-gold"),
    )
    .expect_err("a missing later source must not expose a partial delivery");
    assert_eq!(
        error,
        format!(
            "package source is missing: {}",
            production.join("cover.re.jpg").display()
        )
    );
    assert!(!root.join("3.成品").exists());
    assert!(!production.join("delivery-receipt.json").exists());

    fs::remove_dir_all(root).expect("remove temporary delivery root");
    fs::remove_dir_all(production).expect("remove temporary production");
}

#[test]
fn package_rejects_mismatched_cover_and_video_id_accounts() {
    let root = temporary_directory("delivery-account-root");
    let production = temporary_directory("delivery-account-production");
    write_package_metadata(
        &production,
        "cover-account",
        "video-account",
        "示例",
        "ze-id",
        "re-id",
    );

    let error = package_at_root_with_cover_validation(
        "批量剪辑/test/7.23批量",
        "S01",
        &production,
        &root,
        || Ok("rounded-smoke-gold"),
    )
    .expect_err("different cover and video accounts must not share a delivery");
    assert_eq!(error, "cover and video-id group_name must match");
    assert!(!root.join("3.成品").exists());

    fs::remove_dir_all(root).expect("remove temporary delivery root");
    fs::remove_dir_all(production).expect("remove temporary production");
}

#[test]
fn package_rejects_overlong_delivery_filename_before_creating_output() {
    let root = temporary_directory("delivery-title-root");
    let production = temporary_directory("delivery-title-production");
    write_package_metadata(
        &production,
        "account",
        "account",
        &"皮".repeat(100),
        "ze-id",
        "re-id",
    );

    let error = package_at_root_with_cover_validation(
        "批量剪辑/test/7.23批量",
        "S01",
        &production,
        &root,
        || Ok("rounded-smoke-gold"),
    )
    .expect_err("an overlong filename must fail before delivery output exists");
    assert_eq!(error, "delivery filename is too long");
    assert!(!root.join("3.成品").exists());

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
    let ze_id = "ze-client-42";
    let re_id = "re-client-42";
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

fn write_package_metadata(
    production: &Path,
    cover_group: &str,
    video_group: &str,
    title: &str,
    ze_id: &str,
    re_id: &str,
) {
    fs::write(production.join("quality-gate.json"), r#"{"status":"PASS"}"#)
        .expect("write passing gate");
    fs::write(
        production.join("cover-spec.json"),
        format!(r#"{{"group_name":"{cover_group}"}}"#),
    )
    .expect("write cover spec");
    fs::write(
        production.join("video-id.json"),
        format!(
            r#"{{"group_name":"{video_group}","zh_en_variant_id":"{ze_id}","ru_en_variant_id":"{re_id}","title":"{title}"}}"#
        ),
    )
    .expect("write video ids");
    fs::write(
        production.join("publication.json"),
        r#"{"ze":{"copy":"ZE publication copy"},"re":{"copy":"RE publication copy"}}"#,
    )
    .expect("write publication copy");
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
        fs::read(directory.join(format!("{basename}-封面原图.png"))).expect("read original cover"),
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
