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
    let profile = account_profile(&cover)?;
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
    )
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
    let cover = read_json(&production.join("cover-spec.json"))?;
    cover_text(&cover)?;
    Ok(account_profile(&cover)?.name())
}

#[derive(Clone, Copy, Debug)]
enum CoverProfile {
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
enum CoverLanguage {
    Chinese,
    Overseas,
}

struct CoverText<'a> {
    title: &'a str,
    subtitle: Option<&'a str>,
}

fn font(language: CoverLanguage) -> &'static str {
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
        Some("rounded-smoke-gold") => Ok(CoverProfile::SmokeCard),
        Some("blue-slash") => Ok(CoverProfile::BlueCapsule),
        Some("editorial-gold") => Ok(CoverProfile::EditorialGold),
        Some("navy-orange-impact") => Ok(CoverProfile::OrangeImpact),
        Some("white-card-orange") => Ok(CoverProfile::WhiteCard),
        _ => Err("cover profile_id is unregistered; return this account group for a reference-backed profile".to_owned()),
    }
}

/// Wrap a headline into at most two visually balanced lines.
fn wrap_title(value: &str, language: CoverLanguage) -> String {
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
        CoverLanguage::Chinese => middle,
        CoverLanguage::Overseas => {
            let spaces: Vec<usize> = chars
                .iter()
                .enumerate()
                .filter(|(_, c)| **c == ' ')
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

fn longest_line(value: &str) -> usize {
    value.lines().map(|l| l.chars().count()).max().unwrap_or(0)
}

fn title_size(longest: usize, language: CoverLanguage) -> u32 {
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

fn cover_variant(
    source: &Path,
    timestamp: f64,
    text: CoverText<'_>,
    profile: CoverProfile,
    accent: &str,
    language: CoverLanguage,
    output: &Path,
) -> Result<(), String> {
    let title = wrap_title(text.title, language);
    let lines = title.lines().count().max(1) as u32;
    let size = title_size(longest_line(&title), language);
    let line_height = size + size / 4;
    let font = font(language);
    let title_file = output.with_extension("title.lightflow.txt");
    let subtitle_file = output.with_extension("subtitle.lightflow.txt");
    fs::write(&title_file, &title).map_err(|error| error.to_string())?;
    if let Some(subtitle) = text.subtitle {
        fs::write(&subtitle_file, subtitle).map_err(|error| error.to_string())?;
    }
    let title_path = filter_escape(&title_file);
    let subtitle_path = filter_escape(&subtitle_file);
    let font_path = filter_escape(Path::new(font));
    let has_subtitle = text.subtitle.is_some();

    let mut parts: Vec<String> = Vec::new();
    match profile {
        CoverProfile::SmokeCard => {
            let card_y = 190u32;
            let pad = 60u32;
            let sub_zone = if has_subtitle { 118 } else { 0 };
            let card_h = pad * 2 + lines * line_height + sub_zone;
            parts.push(format!(
                "drawbox=x=64:y={card_y}:w=iw-128:h={card_h}:color=0x0E1116@0.82:t=fill"
            ));
            parts.push(format!(
                "drawbox=x=64:y={card_y}:w=iw-128:h=7:color={accent}@0.95:t=fill"
            ));
            parts.push(format!(
                "drawtext=fontfile='{font_path}':textfile='{title_path}':fontcolor=white:fontsize={size}:line_spacing=16:x=(w-text_w)/2:y={}",
                card_y + pad - 8
            ));
            if has_subtitle {
                let divider_y = card_y + pad + lines * line_height + 18;
                parts.push(format!(
                    "drawbox=x=(iw-220)/2:y={divider_y}:w=220:h=4:color={accent}@0.85:t=fill"
                ));
                parts.push(format!(
                    "drawtext=fontfile='{font_path}':textfile='{subtitle_path}':fontcolor={accent}:fontsize=46:x=(w-text_w)/2:y={}",
                    divider_y + 26
                ));
            }
        }
        CoverProfile::BlueCapsule => {
            let cap_y = 150u32;
            let cap_h = 70 + lines * line_height;
            parts.push(format!(
                "drawbox=x=96:y={cap_y}:w=iw-192:h={cap_h}:color=0x1667E0@0.95:t=fill"
            ));
            parts.push(format!(
                "drawbox=x=96:y={cap_y}:w=iw-192:h={cap_h}:color=white@0.9:t=6"
            ));
            parts.push(format!(
                "drawtext=fontfile='{font_path}':textfile='{title_path}':fontcolor=white:fontsize={size}:line_spacing=14:x=(w-text_w)/2:y={}",
                cap_y + 34
            ));
            if has_subtitle {
                let bar_y = cap_y + cap_h + 20;
                parts.push(format!(
                    "drawbox=x=200:y={bar_y}:w=iw-400:h=88:color=white@0.94:t=fill"
                ));
                parts.push(format!(
                    "drawtext=fontfile='{font_path}':textfile='{subtitle_path}':fontcolor=0x114A9E:fontsize=44:x=(w-text_w)/2:y={}",
                    bar_y + 22
                ));
            }
        }
        CoverProfile::EditorialGold => {
            let panel_h = 220 + lines * line_height + if has_subtitle { 96 } else { 0 };
            parts.push(format!(
                "drawbox=x=0:y=0:w=iw:h={panel_h}:color=0xFAF8F3@0.96:t=fill"
            ));
            parts.push(format!(
                "drawbox=x=0:y={panel_h}:w=iw:h=9:color={accent}@0.98:t=fill"
            ));
            parts.push(format!(
                "drawbox=x=72:y=96:w=150:h=10:color={accent}@0.98:t=fill"
            ));
            parts.push(format!(
                "drawtext=fontfile='{font_path}':textfile='{title_path}':fontcolor=0x171717:fontsize={size}:line_spacing=14:x=72:y=140"
            ));
            if has_subtitle {
                parts.push(format!(
                    "drawtext=fontfile='{font_path}':textfile='{subtitle_path}':fontcolor=0xA87718:fontsize=46:x=74:y={}",
                    150 + lines * line_height + 28
                ));
            }
        }
        CoverProfile::OrangeImpact => {
            parts.push(format!(
                "drawtext=fontfile='{font_path}':textfile='{title_path}':fontcolor=0xFFC02D:bordercolor=0x0C2D5B:borderw=10:fontsize={size}:line_spacing=16:x=(w-text_w)/2:y=150"
            ));
            if has_subtitle {
                let chip_y = 170 + lines * line_height + 40;
                parts.push(format!(
                    "drawbox=x=210:y={chip_y}:w=iw-420:h=92:color=0xFF7A00@0.95:t=fill"
                ));
                parts.push(format!(
                    "drawtext=fontfile='{font_path}':textfile='{subtitle_path}':fontcolor=white:fontsize=44:x=(w-text_w)/2:y={}",
                    chip_y + 24
                ));
            }
        }
        CoverProfile::WhiteCard => {
            let card_y = 110u32;
            let pad = 56u32;
            let sub_zone = if has_subtitle { 110 } else { 0 };
            let card_h = pad * 2 + lines * line_height + sub_zone;
            parts.push(format!(
                "drawbox=x=52:y={card_y}:w=iw-104:h={card_h}:color=0xFAF8F3@0.94:t=fill"
            ));
            parts.push(format!(
                "drawbox=x=52:y={}:w=iw-104:h=8:color=0xFF7A00@0.96:t=fill",
                card_y + card_h - 8
            ));
            parts.push(format!(
                "drawtext=fontfile='{font_path}':textfile='{title_path}':fontcolor=0x1B1B1B:fontsize={size}:line_spacing=14:x=(w-text_w)/2:y={}",
                card_y + pad - 6
            ));
            if has_subtitle {
                parts.push(format!(
                    "drawtext=fontfile='{font_path}':textfile='{subtitle_path}':fontcolor=0xB35400:fontsize=46:x=(w-text_w)/2:y={}",
                    card_y + pad + lines * line_height + 26
                ));
            }
        }
    }
    let filter = parts.join(",");

    let staged = output.with_file_name(format!(
        ".{}.lightflow-staged.jpg",
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("cover output has no valid filename")?
    ));
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
}
