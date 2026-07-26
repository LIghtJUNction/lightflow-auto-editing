use super::cover_render::cover_variant;
use super::read_json;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn render_covers(
    source_root: &Path,
    production: &Path,
    edl: &Value,
    _captions: &Value,
) -> Result<(), String> {
    let cover = read_json(&production.join("cover-spec.json"))?;
    // Validate both publication-language headlines before touching either
    // output. A malformed spec must leave previously approved covers intact.
    let text = cover_text(&cover)?;
    let source = cover
        .pointer("/source/path")
        .and_then(Value::as_str)
        .map(|value| PathBuf::from(value.replacen("/srv/xry/", "/srv/", 1)))
        .filter(|path| path.is_file())
        .or_else(|| {
            edl.pointer("/video_segments/0/source")
                .and_then(Value::as_str)
                .map(|name| source_root.join(name))
        })
        .ok_or("cover source is missing")?;
    let timestamp = cover
        .pointer("/source/timestamp_seconds")
        .and_then(Value::as_f64)
        .unwrap_or(3.0);
    let (profile, reference) = account_profile_at_reference_root(&cover, Path::new("/srv/0.参考"))?;
    let accent = accent_for(&cover);
    cover_variant(
        &source,
        timestamp,
        CoverText {
            title: text.zh,
            subtitle: text.zh_subtitle,
        },
        profile,
        accent,
        CoverLanguage::Chinese,
        &production.join("cover.ze.jpg"),
    )?;
    cover_variant(
        &source,
        timestamp,
        CoverText {
            title: text.ru,
            subtitle: text.ru_subtitle,
        },
        profile,
        accent,
        CoverLanguage::Overseas,
        &production.join("cover.re.jpg"),
    )?;
    materialize_cover_original(&reference, production)
}

#[derive(Debug)]
struct PublicationCoverText<'a> {
    zh: &'a str,
    ru: &'a str,
    zh_subtitle: Option<&'a str>,
    ru_subtitle: Option<&'a str>,
}

fn cover_text(cover: &Value) -> Result<PublicationCoverText<'_>, String> {
    let zh = required_headline(cover, "headline_zh", contains_cjk, "CJK")?;
    let ru = required_headline(cover, "headline_ru", contains_cyrillic, "Cyrillic")?;
    Ok(PublicationCoverText {
        zh,
        ru,
        zh_subtitle: cover
            .get("subheadline_zh")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty()),
        ru_subtitle: cover
            .get("subheadline")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty()),
    })
}

fn required_headline<'a>(
    cover: &'a Value,
    field: &str,
    language_check: fn(&str) -> bool,
    language_name: &str,
) -> Result<&'a str, String> {
    let value = cover
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("cover {field} is required"))?;
    if language_check(value) {
        Ok(value)
    } else {
        Err(format!(
            "cover {field} must contain {language_name} characters"
        ))
    }
}

fn contains_cjk(value: &str) -> bool {
    value.chars().any(|character| {
        matches!(character,
            '\u{3400}'..='\u{4DBF}'
                | '\u{4E00}'..='\u{9FFF}'
                | '\u{F900}'..='\u{FAFF}'
                | '\u{20000}'..='\u{2EBEF}'
        )
    })
}

fn contains_cyrillic(value: &str) -> bool {
    value.chars().any(|character| {
        matches!(character,
            '\u{0400}'..='\u{052F}'
                | '\u{2DE0}'..='\u{2DFF}'
                | '\u{A640}'..='\u{A69F}'
        )
    })
}

pub(super) fn validate_cover_spec(production: &Path) -> Result<&'static str, String> {
    validate_cover_spec_at_reference_root(production, Path::new("/srv/0.参考"))
}

#[cfg(test)]
pub(super) fn validate_cover_spec_with_reference_root(
    production: &Path,
    reference_root: &Path,
) -> Result<&'static str, String> {
    validate_cover_spec_at_reference_root(production, reference_root)
}

#[cfg(test)]
pub(super) fn materialize_cover_original_with_reference_root(
    production: &Path,
    reference_root: &Path,
) -> Result<(), String> {
    let cover = read_json(&production.join("cover-spec.json"))?;
    let (_, reference) = account_profile_at_reference_root(&cover, reference_root)?;
    materialize_cover_original(&reference, production)
}

fn validate_cover_spec_at_reference_root(
    production: &Path,
    reference_root: &Path,
) -> Result<&'static str, String> {
    let cover = read_json(&production.join("cover-spec.json"))?;
    cover_text(&cover)?;
    let (profile, _) = account_profile_at_reference_root(&cover, reference_root)?;
    Ok(profile.name())
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CoverProfile {
    /// Centered smoke-glass card, bold white title, accent hairline + subtitle.
    SmokeCard,
    /// Blue capsule headline with a white subtitle bar (柴油客货严选 sample).
    BlueCapsule,
    /// White editorial top panel with dark title and gold accents (走全球).
    EditorialGold,
    /// Big outlined impact type with an orange chip (线师傅 sample).
    OrangeImpact,
    /// Light card on top with dark title and orange accent.
    WhiteCard,
}

impl CoverProfile {
    fn name(self) -> &'static str {
        match self {
            Self::SmokeCard => "rounded-smoke-gold",
            Self::BlueCapsule => "blue-slash",
            Self::EditorialGold => "editorial-gold",
            Self::OrangeImpact => "navy-orange-impact",
            Self::WhiteCard => "white-card-orange",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum CoverLanguage {
    Chinese,
    Overseas,
}

pub(super) struct CoverText<'a> {
    pub(super) title: &'a str,
    pub(super) subtitle: Option<&'a str>,
}

pub(super) fn font(language: CoverLanguage) -> &'static str {
    match language {
        CoverLanguage::Chinese => "/usr/share/fonts/noto-cjk/NotoSansCJK-Black.ttc",
        CoverLanguage::Overseas => "/usr/share/fonts/noto/NotoSans-Black.ttf",
    }
}

fn accent_for(cover: &Value) -> &'static str {
    match cover
        .get("group_name")
        .and_then(Value::as_str)
        .unwrap_or("")
    {
        "1-5万二手皮卡" => "0xD6A62C",
        "专搞皮卡" => "0x3D8BFF",
        "洒水车" => "0x2FC4B2",
        "阳哥大皮卡" => "0xFF8A00",
        "皮卡严选 走全球" => "0xC99A2E",
        "个人生活号" => "0xE8C36A",
        _ => "0xD6A62C",
    }
}

#[cfg(test)]
fn account_profile(cover: &Value) -> Result<CoverProfile, String> {
    account_profile_at_reference_root(cover, Path::new("/srv/0.参考")).map(|(profile, _)| profile)
}

fn account_profile_at_reference_root(
    cover: &Value,
    reference_root: &Path,
) -> Result<(CoverProfile, PathBuf), String> {
    let group = cover
        .get("group_name")
        .and_then(Value::as_str)
        .ok_or("cover group_name is missing")?;
    let reference = cover
        .pointer("/style_references/0/path")
        .and_then(Value::as_str)
        .ok_or("cover requires a style reference")?;
    let reference = PathBuf::from(reference.replacen("/srv/xry/", "/srv/", 1));
    let expected = reference_root.join(group);
    if !reference.is_file() || !reference.starts_with(&expected) {
        return Err(
            "cover reference must be an existing file under its account group in /srv/0.参考"
                .to_owned(),
        );
    }
    let profile = match cover.get("profile_id").and_then(Value::as_str) {
        Some("rounded-smoke-gold") => CoverProfile::SmokeCard,
        Some("blue-slash") => CoverProfile::BlueCapsule,
        Some("editorial-gold") => CoverProfile::EditorialGold,
        Some("navy-orange-impact") => CoverProfile::OrangeImpact,
        Some("white-card-orange") => CoverProfile::WhiteCard,
        _ => return Err("cover profile_id is unregistered; return this account group for a reference-backed profile".to_owned()),
    };
    Ok((profile, reference))
}

fn materialize_cover_original(reference: &Path, production: &Path) -> Result<(), String> {
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

/// Wrap a headline into at most two visually balanced lines.
pub(super) fn wrap_title(value: &str, language: CoverLanguage) -> String {
    if value.contains('\n') {
        return value.to_owned();
    }
    let chars: Vec<char> = value.chars().collect();
    let limit = match language {
        CoverLanguage::Chinese => 9,
        CoverLanguage::Overseas => 16,
    };
    if chars.len() <= limit {
        return value.to_owned();
    }
    let middle = chars.len() / 2;
    let split = match language {
        CoverLanguage::Chinese => {
            // Never split inside an ASCII run (model names like D-MAX, GL8).
            let is_ascii_part =
                |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '&';
            let candidates: Vec<usize> = (1..chars.len())
                .filter(|&i| !(is_ascii_part(chars[i - 1]) && is_ascii_part(chars[i])))
                .collect();
            candidates
                .iter()
                .min_by_key(|i| i.abs_diff(middle))
                .copied()
                .unwrap_or(middle)
        }
        CoverLanguage::Overseas => {
            // Break only at spaces that don't sit inside a digit group (10 000).
            let spaces: Vec<usize> = chars
                .iter()
                .enumerate()
                .filter(|&(i, c)| {
                    *c == ' '
                        && !(i > 0
                            && i + 1 < chars.len()
                            && chars[i - 1].is_ascii_digit()
                            && chars[i + 1].is_ascii_digit())
                })
                .map(|(i, _)| i)
                .collect();
            spaces
                .iter()
                .min_by_key(|i| i.abs_diff(middle))
                .copied()
                .unwrap_or(middle)
        }
    };
    let head: String = chars[..split].iter().collect();
    let tail: String = chars[split..].iter().collect();
    format!("{}\n{}", head.trim_end(), tail.trim_start())
}

pub(super) fn longest_line(value: &str) -> usize {
    value.lines().map(|l| l.chars().count()).max().unwrap_or(0)
}

pub(super) fn title_size(longest: usize, language: CoverLanguage) -> u32 {
    match language {
        CoverLanguage::Chinese => match longest {
            0..=6 => 116,
            7..=8 => 102,
            9..=10 => 92,
            _ => 82,
        },
        CoverLanguage::Overseas => match longest {
            0..=10 => 84,
            11..=14 => 72,
            15..=18 => 62,
            _ => 54,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_missing_account_reference() {
        let error = account_profile(&serde_json::json!({
            "group_name": "unknown",
            "profile_id": "rounded-smoke-gold",
            "style_references": []
        }))
        .expect_err("a profile must have a reference");
        assert_eq!(error, "cover requires a style reference");
    }

    #[test]
    fn rejects_unregistered_profile_before_rendering() {
        let error = account_profile(&serde_json::json!({
            "group_name": "unknown",
            "profile_id": "not-registered",
            "style_references": [{"path": "/not-a-reference.png"}]
        }))
        .expect_err("unknown profile must not render");
        assert!(error.contains("cover reference"));
    }

    #[test]
    fn detects_required_cjk_and_cyrillic_headlines() {
        assert!(contains_cjk("皮卡精选"));
        assert!(!contains_cjk("Pickup review"));
        assert!(contains_cyrillic("Новый пикап"));
        assert!(!contains_cyrillic("Pickup review"));
    }

    #[test]
    fn wraps_long_chinese_titles_into_two_lines() {
        let wrapped = wrap_title("无报废年限厢式皮卡现车齐全", CoverLanguage::Chinese);
        assert_eq!(wrapped.lines().count(), 2);
        let short = wrap_title("近百万公里", CoverLanguage::Chinese);
        assert_eq!(short.lines().count(), 1);
        let manual = wrap_title("第一行\n第二行", CoverLanguage::Chinese);
        assert_eq!(manual.lines().count(), 2);
    }

    #[test]
    fn wraps_russian_titles_at_spaces() {
        let wrapped = wrap_title("ПИКАПЫ-ФУРГОНЫ БЕЗ СРОКА СПИСАНИЯ", CoverLanguage::Overseas);
        assert_eq!(wrapped.lines().count(), 2);
        for line in wrapped.lines() {
            assert!(!line.starts_with(' ') && !line.ends_with(' '));
        }
    }

    #[test]
    fn rejects_missing_or_wrong_language_headlines_before_render() {
        let missing_zh = cover_text(&serde_json::json!({"headline_ru": "Новый пикап"}))
            .expect_err("ZE headline is mandatory");
        assert_eq!(missing_zh, "cover headline_zh is required");

        let wrong_ru = cover_text(&serde_json::json!({
            "headline_zh": "皮卡精选",
            "headline_ru": "Pickup review"
        }))
        .expect_err("RE headline must contain Cyrillic");
        assert_eq!(
            wrong_ru,
            "cover headline_ru must contain Cyrillic characters"
        );
    }

    #[test]
    fn rejects_non_png_reference_for_original_cover_artifact() {
        let root = temporary_directory("original-cover-extension");
        let reference = root.join("reference.jpg");
        let production = root.join("production");
        fs::write(&reference, b"not a PNG").expect("write non-PNG reference");

        let error = materialize_cover_original(&reference, &production)
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

        fs::remove_dir_all(root).expect("remove temporary cover root");
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-xry-worker-covers-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary cover directory");
        path
    }
}
