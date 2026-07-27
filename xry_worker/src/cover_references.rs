use std::fs;
use std::path::{Component, Path, PathBuf};

const INVALID_REFERENCE: &str =
    "cover reference must be an existing file under its account group in /srv/0.参考";

pub(super) fn canonical_account_reference(
    group: &str,
    raw_reference: &str,
    reference_root: &Path,
) -> Result<PathBuf, String> {
    let reference_root = reference_root
        .canonicalize()
        .map_err(|_| invalid_reference())?;
    let group_directory = canonical_group_directory(&reference_root, group)?;
    let reference = PathBuf::from(raw_reference.replacen("/srv/xry/", "/srv/", 1))
        .canonicalize()
        .map_err(|_| invalid_reference())?;
    if !reference.is_file() || !reference.starts_with(&group_directory) {
        return Err(invalid_reference());
    }
    Ok(reference)
}

pub(super) fn materialize_original(reference: &Path, production: &Path) -> Result<(), String> {
    let is_png = reference
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"));
    if !is_png {
        return Err(
            "cover style reference must be a PNG before materializing cover-original.png"
                .to_owned(),
        );
    }
    atomic_copy(reference, &production.join("cover-original.png"))
}

fn canonical_group_directory(reference_root: &Path, group: &str) -> Result<PathBuf, String> {
    if !is_single_normal_component(group) {
        return Err(invalid_reference());
    }
    let group_directory = reference_root
        .join(group)
        .canonicalize()
        .map_err(|_| invalid_reference())?;
    if !group_directory.starts_with(reference_root) {
        return Err(invalid_reference());
    }
    Ok(group_directory)
}

fn is_single_normal_component(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn invalid_reference() -> String {
    INVALID_REFERENCE.to_owned()
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_file() {
        return Err(format!("cover reference is missing: {}", source.display()));
    }
    let parent = destination
        .parent()
        .ok_or("cover-original destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staged = parent.join(format!(
        ".{}.lightflow-staged",
        destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("cover-original destination has no valid filename")?
    ));
    if let Err(error) = fs::copy(source, &staged) {
        let _ = fs::remove_file(&staged);
        return Err(error.to_string());
    }
    if let Err(error) = fs::rename(&staged, destination) {
        let _ = fs::remove_file(&staged);
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_reference_path_that_escapes_its_account_directory() {
        let root = temporary_directory("reference-traversal");
        let reference_root = root.join("0.参考");
        let group_directory = reference_root.join("account");
        let outside = reference_root.join("outside.png");
        fs::create_dir_all(&group_directory).expect("create account directory");
        fs::write(&outside, b"outside reference").expect("write outside reference");
        let escaping = group_directory.join("..").join("outside.png");

        let error = canonical_account_reference(
            "account",
            escaping.to_str().expect("temporary path is UTF-8"),
            &reference_root,
        )
        .expect_err("a reference outside its account directory must be rejected");
        assert_eq!(error, INVALID_REFERENCE);

        fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[test]
    fn rejects_group_name_with_multiple_path_components() {
        let root = temporary_directory("group-traversal");
        let reference_root = root.join("0.参考");
        let reference = reference_root.join("other").join("reference.png");
        fs::create_dir_all(reference.parent().expect("reference parent"))
            .expect("create reference directory");
        fs::write(&reference, b"reference").expect("write reference");

        let error = canonical_account_reference(
            "account/../other",
            reference.to_str().expect("temporary path is UTF-8"),
            &reference_root,
        )
        .expect_err("a multi-component account group must be rejected");
        assert_eq!(error, INVALID_REFERENCE);

        fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_reference_symlink_that_escapes_its_account_directory() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("reference-symlink");
        let reference_root = root.join("0.参考");
        let group_directory = reference_root.join("account");
        let outside = root.join("outside.png");
        fs::create_dir_all(&group_directory).expect("create account directory");
        fs::write(&outside, b"outside reference").expect("write outside reference");
        let link = group_directory.join("reference.png");
        symlink(&outside, &link).expect("create escaping symlink");

        let error = canonical_account_reference(
            "account",
            link.to_str().expect("temporary path is UTF-8"),
            &reference_root,
        )
        .expect_err("an escaping reference symlink must be rejected");
        assert_eq!(error, INVALID_REFERENCE);

        fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_reference_through_directory_symlink_that_escapes_its_account_directory() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("reference-directory-symlink");
        let reference_root = root.join("0.参考");
        let group_directory = reference_root.join("account");
        let outside_directory = root.join("outside");
        fs::create_dir_all(&group_directory).expect("create account directory");
        fs::create_dir_all(&outside_directory).expect("create outside directory");
        fs::write(
            outside_directory.join("reference.png"),
            b"outside reference",
        )
        .expect("write outside reference");
        let link = group_directory.join("linked");
        symlink(&outside_directory, &link).expect("create escaping directory symlink");

        let error = canonical_account_reference(
            "account",
            link.join("reference.png")
                .to_str()
                .expect("temporary path is UTF-8"),
            &reference_root,
        )
        .expect_err("a reference through an escaping directory symlink must be rejected");
        assert_eq!(error, INVALID_REFERENCE);

        fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[test]
    fn rejects_non_png_reference_for_original_cover_artifact() {
        let root = temporary_directory("original-cover-extension");
        let reference = root.join("reference.jpg");
        let production = root.join("production");
        fs::write(&reference, b"not a PNG").expect("write non-PNG reference");

        let error = materialize_original(&reference, &production)
            .expect_err("a non-PNG reference must not be renamed as a PNG");
        assert_eq!(
            error,
            "cover style reference must be a PNG before materializing cover-original.png"
        );
        assert!(!production.join("cover-original.png").exists());
        assert!(
            !production
                .join(".cover-original.png.lightflow-staged")
                .exists()
        );

        fs::remove_dir_all(root).expect("remove temporary root");
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-xry-worker-cover-references-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary directory");
        path
    }
}
