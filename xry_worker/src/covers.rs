use super::{read_json, run_command};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
        .map(PathBuf::from)
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
    let profile = account_profile(&cover)?;
    cover_variant(
        &source,
        timestamp,
        CoverText {
            title: text.zh,
            subtitle: None,
            footer: None,
        },
        profile,
        CoverLanguage::Chinese,
        &production.join("cover.ze.jpg"),
    )?;
    cover_variant(
        &source,
        timestamp,
        CoverText {
            title: text.ru,
            subtitle: text.ru_subtitle,
            footer: text.ru_footer,
        },
        profile,
        CoverLanguage::Overseas,
        &production.join("cover.re.jpg"),
    )
}

#[derive(Debug)]
struct PublicationCoverText<'a> {
    zh: &'a str,
    ru: &'a str,
    ru_subtitle: Option<&'a str>,
    ru_footer: Option<&'a str>,
}

fn cover_text(cover: &Value) -> Result<PublicationCoverText<'_>, String> {
    let zh = required_headline(cover, "headline_zh", contains_cjk, "CJK")?;
    let ru = required_headline(cover, "headline_ru", contains_cyrillic, "Cyrillic")?;
    Ok(PublicationCoverText {
        zh,
        ru,
        ru_subtitle: cover.get("subheadline").and_then(Value::as_str),
        ru_footer: cover.get("footer_fact_2").and_then(Value::as_str),
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
    let cover = read_json(&production.join("cover-spec.json"))?;
    cover_text(&cover)?;
    Ok(account_profile(&cover)?.name())
}

#[derive(Clone, Copy, Debug)]
enum CoverProfile {
    RoundedSmokeGold,
    BlueSlash,
    EditorialGold,
    NavyOrangeImpact,
    WhiteCardOrange,
}

impl CoverProfile {
    fn name(self) -> &'static str {
        match self {
            Self::RoundedSmokeGold => "rounded-smoke-gold",
            Self::BlueSlash => "blue-slash",
            Self::EditorialGold => "editorial-gold",
            Self::NavyOrangeImpact => "navy-orange-impact",
            Self::WhiteCardOrange => "white-card-orange",
        }
    }

    fn card(
        self,
        language: CoverLanguage,
    ) -> (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        u32,
        &'static str,
    ) {
        match self {
            Self::RoundedSmokeGold => (
                "70",
                "250",
                "iw-140",
                "0x101214@0.84",
                if matches!(language, CoverLanguage::Chinese) {
                    112
                } else {
                    82
                },
                font(language),
            ),
            Self::BlueSlash => ("0", "0", "iw*0.68", "0xF7F8FA@0.92", 62, font(language)),
            Self::EditorialGold => (
                "56",
                "ih-310",
                "iw-112",
                "0x202020@0.94",
                58,
                font(language),
            ),
            Self::NavyOrangeImpact => ("0", "ih-350", "iw", "0x10253F@0.94", 64, font(language)),
            Self::WhiteCardOrange => ("48", "100", "iw-96", "0xFAF8F3@0.94", 60, font(language)),
        }
    }
}

#[derive(Clone, Copy)]
enum CoverLanguage {
    Chinese,
    Overseas,
}

struct CoverText<'a> {
    title: &'a str,
    subtitle: Option<&'a str>,
    footer: Option<&'a str>,
}

fn font(language: CoverLanguage) -> &'static str {
    match language {
        CoverLanguage::Chinese => "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        CoverLanguage::Overseas => "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    }
}

fn account_profile(cover: &Value) -> Result<CoverProfile, String> {
    let group = cover
        .get("group_name")
        .and_then(Value::as_str)
        .ok_or("cover group_name is missing")?;
    let reference = cover
        .pointer("/style_references/0/path")
        .and_then(Value::as_str)
        .ok_or("cover requires a style reference")?;
    let reference = PathBuf::from(reference.replacen("/srv/xry/", "/srv/", 1));
    let expected = Path::new("/srv/0.参考").join(group);
    if !reference.is_file() || !reference.starts_with(&expected) {
        return Err(
            "cover reference must be an existing file under its account group in /srv/0.参考"
                .to_owned(),
        );
    }
    match cover.get("profile_id").and_then(Value::as_str) {
        Some("rounded-smoke-gold") => Ok(CoverProfile::RoundedSmokeGold),
        Some("blue-slash") => Ok(CoverProfile::BlueSlash),
        Some("editorial-gold") => Ok(CoverProfile::EditorialGold),
        Some("navy-orange-impact") => Ok(CoverProfile::NavyOrangeImpact),
        Some("white-card-orange") => Ok(CoverProfile::WhiteCardOrange),
        _ => Err("cover profile_id is unregistered; return this account group for a reference-backed profile".to_owned()),
    }
}

fn cover_variant(
    source: &Path,
    timestamp: f64,
    text: CoverText<'_>,
    profile: CoverProfile,
    language: CoverLanguage,
    output: &Path,
) -> Result<(), String> {
    let (x, y, width, color, font_size, font) = profile.card(language);
    let title_file = output.with_extension("title.lightflow.txt");
    let subtitle_file = output.with_extension("subtitle.lightflow.txt");
    let footer_file = output.with_extension("footer.lightflow.txt");
    let staged = output.with_file_name(format!(
        ".{}.lightflow-staged.jpg",
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("cover output has no valid filename")?
    ));
    fs::write(&title_file, text.title).map_err(|error| error.to_string())?;
    let filter = if matches!(profile, CoverProfile::EditorialGold) {
        fs::write(&subtitle_file, text.subtitle.unwrap_or_default())
            .map_err(|error| error.to_string())?;
        fs::write(&footer_file, text.footer.unwrap_or_default())
            .map_err(|error| error.to_string())?;
        format!(
            "drawbox=x=0:y=0:w=iw:h=410:color=0xFAF8F3@0.96:t=fill,drawbox=x=0:y=ih-280:w=iw:h=280:color=0x171717@0.94:t=fill,drawbox=x=0:y=405:w=iw:h=8:color=0xC99A2E@0.98:t=fill,drawtext=fontfile='{}':textfile='{}':fontcolor=0x171717:fontsize=78:x=72:y=190,drawtext=fontfile='{}':textfile='{}':fontcolor=0xA87718:fontsize=42:x=74:y=305,drawtext=fontfile='{}':textfile='{}':fontcolor=0xD7A82B:fontsize=52:x=74:y=h-165",
            filter_escape(Path::new(font)),
            filter_escape(&title_file),
            filter_escape(Path::new(font)),
            filter_escape(&subtitle_file),
            filter_escape(Path::new(font)),
            filter_escape(&footer_file),
        )
    } else {
        format!(
            "drawbox=x={x}:y={y}:w={width}:h={}:color={color}:t=fill,drawbox=x={x}:y={y}:w={width}:h={}:color={}:t=5,drawtext=fontfile='{}':textfile='{}':fontcolor={}:fontsize={font_size}:x={}:y={}",
            if matches!(profile, CoverProfile::RoundedSmokeGold) {
                "330"
            } else {
                "230"
            },
            if matches!(profile, CoverProfile::RoundedSmokeGold) {
                "330"
            } else {
                "230"
            },
            if matches!(profile, CoverProfile::RoundedSmokeGold) {
                "0xD6A62C@0.94"
            } else {
                "black@0"
            },
            filter_escape(Path::new(font)),
            filter_escape(&title_file),
            if matches!(profile, CoverProfile::WhiteCardOrange) {
                "0x1B1B1B"
            } else {
                "white"
            },
            if matches!(profile, CoverProfile::BlueSlash) {
                "58"
            } else {
                "92"
            },
            if matches!(profile, CoverProfile::RoundedSmokeGold) {
                "330"
            } else if matches!(
                profile,
                CoverProfile::NavyOrangeImpact | CoverProfile::EditorialGold
            ) {
                "h-290"
            } else {
                "150"
            },
        )
    };
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-y",
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-ss",
            &format!("{timestamp:.6}"),
            "-i",
        ])
        .arg(source)
        .args(["-vf", &filter, "-frames:v", "1", "-q:v", "2"])
        .arg(&staged);
    let result = run_command(&mut command, "render account cover");
    let _ = fs::remove_file(&title_file);
    let _ = fs::remove_file(&subtitle_file);
    let _ = fs::remove_file(&footer_file);
    result?;
    fs::rename(staged, output).map_err(|error| error.to_string())
}

fn filter_escape(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
        .replace(',', "\\,")
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
    fn rejects_missing_or_wrong_language_headlines_before_render() {
        let missing_zh = cover_text(&serde_json::json!({"headline_ru": "Новый пикап"}))
            .expect_err("ZE headline is mandatory");
        assert_eq!(missing_zh, "cover headline_zh is required");

        let wrong_zh = cover_text(&serde_json::json!({
            "headline_zh": "Pickup review",
            "headline_ru": "Новый пикап"
        }))
        .expect_err("ZE headline must contain CJK");
        assert_eq!(wrong_zh, "cover headline_zh must contain CJK characters");

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
    fn invalid_headlines_leave_existing_covers_untouched_before_ffmpeg() {
        let production = temporary_production("invalid-cover-text");
        let ze = production.join("cover.ze.jpg");
        let re = production.join("cover.re.jpg");
        fs::write(&ze, "approved-ze-cover").expect("write ZE sentinel");
        fs::write(&re, "approved-RE-cover").expect("write RE sentinel");

        for (specification, expected_error) in [
            (
                serde_json::json!({"headline_ru": "Новый пикап"}),
                "cover headline_zh is required",
            ),
            (
                serde_json::json!({
                    "headline_zh": "皮卡精选",
                    "headline_ru": "Pickup review"
                }),
                "cover headline_ru must contain Cyrillic characters",
            ),
        ] {
            fs::write(
                production.join("cover-spec.json"),
                serde_json::to_vec(&specification).expect("encode invalid cover spec"),
            )
            .expect("write invalid cover spec");

            let error = render_covers(
                Path::new("/missing-source-root"),
                &production,
                &serde_json::json!({"video_segments": []}),
                &serde_json::json!({}),
            )
            .expect_err("invalid cover text must reject before ffmpeg");
            assert_eq!(error, expected_error);
            assert_eq!(
                fs::read_to_string(&ze).expect("read ZE sentinel"),
                "approved-ze-cover"
            );
            assert_eq!(
                fs::read_to_string(&re).expect("read RE sentinel"),
                "approved-RE-cover"
            );
        }

        fs::remove_dir_all(production).expect("remove temporary production");
    }

    fn temporary_production(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-xry-worker-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temporary production");
        path
    }
}
