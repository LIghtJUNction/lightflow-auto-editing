use std::fs;
use std::path::Path;
use std::process::Command;

use super::covers::{
    CoverLanguage, CoverProfile, CoverText, font, longest_line, title_size, wrap_title,
};
use super::run_command;

pub(super) fn cover_variant(
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
