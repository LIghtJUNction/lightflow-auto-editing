use super::{field_number, field_text, run_command, timeline_duration};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

pub(super) fn burn(master: &Path, ass: &Path, output: &Path) -> Result<(), String> {
    let filter = ass
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'");
    let mut command = Command::new("ffmpeg");
    command
        .args(["-y", "-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(master)
        .args([
            "-vf",
            &format!("ass='{filter}'"),
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(output);
    run_command(&mut command, "burn subtitles")
}

/// Inline ASS override colours (&HBBGGRR&).
const GOLD: &str = "&H2CA6D6&";
const RU_YELLOW: &str = "&H00D8FF&";

pub(super) fn ass(edl: &Value, events: &[Value], main: &str, sub: &str) -> Result<String, String> {
    let duration = timeline_duration(edl)?;
    // Big, high-contrast subtitles: heavy main line with thick outline and
    // soft shadow so they stay readable on any footage.
    let mut output = String::from(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920\n\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\nStyle: Main,Noto Sans CJK SC,116,&H00FFFFFF,&H000000FF,&H00101010,&H90000000,-1,0,0,0,100,100,1,0,1,6,2,2,48,48,210,1\nStyle: Sub,Noto Sans,60,&H00FFFFFF,&H000000FF,&H00101010,&H90000000,-1,0,0,0,100,100,0,0,1,4,2,2,48,48,110,1\n\n[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\n",
    );
    let mut previous_end = 0.0_f64;
    for event in events {
        let item = event.as_object().ok_or("caption event must be object")?;
        let start = field_number(item, "start")?;
        let end = field_number(item, "end")?;
        if !(end > start && start >= -0.001) {
            return Err("caption event bounds are invalid".to_owned());
        }
        if start + 0.001 < previous_end {
            return Err("caption events must not overlap".to_owned());
        }
        if end > duration + 0.25 {
            return Err("caption event is beyond the frozen timeline".to_owned());
        }
        previous_end = end;
        let primary = field_text(item, main)?;
        let secondary = field_text(item, sub)?;
        let keywords: Vec<&str> = item
            .get("keywords")
            .and_then(|value| value.get(main))
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        // RE contract: Russian main line is a uniform yellow; keywords only
        // bold. ZE keeps a white main line with gold keyword emphasis.
        let (line_color, keyword_color, base_bold) = if main == "ru" {
            (Some(RU_YELLOW), None, false)
        } else {
            (None, Some(GOLD), true)
        };
        let main_text = emphasize(primary, &keywords, keyword_color, line_color, base_bold);
        let color_prefix = line_color.map_or(String::new(), |c| {
            format!("{{\\b{}\\c{c}}}", if base_bold { 1 } else { 0 })
        });
        let fade = fade_milliseconds(start, end);
        output.push_str(&format!(
            "Dialogue: 0,{},{},Main,,0,0,0,,{{\\fad({fade},{fade})}}{}{}\\N{{\\rSub}}{{\\fad({fade},{fade})}}{}\n",
            timestamp(start.max(0.0)),
            timestamp(end.min(duration)),
            color_prefix,
            main_text,
            escape_ass(secondary)
        ));
    }
    Ok(output)
}

/// Wrap each keyword occurrence with bold (and colour when given), restoring
/// the base line style afterwards.
fn emphasize(
    text: &str,
    keywords: &[&str],
    keyword_color: Option<&str>,
    line_color: Option<&str>,
    base_bold: bool,
) -> String {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for keyword in keywords {
        if keyword.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(position) = text[from..].find(keyword) {
            let begin = from + position;
            let finish = begin + keyword.len();
            ranges.push((begin, finish));
            from = finish;
        }
    }
    if ranges.is_empty() {
        return escape_ass(text);
    }
    ranges.sort_unstable();
    let mut merged = Vec::with_capacity(ranges.len());
    for (begin, finish) in ranges {
        if let Some((_, previous_finish)) = merged.last_mut()
            && begin <= *previous_finish
        {
            *previous_finish = (*previous_finish).max(finish);
        } else {
            merged.push((begin, finish));
        }
    }
    let restore_color = line_color.map_or("\\c&HFFFFFF&".to_owned(), |c| format!("\\c{c}"));
    let restore_bold = if base_bold { "\\b1" } else { "\\b0" };
    let start_tag = match keyword_color {
        Some(color) => format!("{{\\b1\\c{color}}}"),
        None => "{\\b1}".to_owned(),
    };
    let end_tag = match keyword_color {
        Some(_) => format!("{{{restore_bold}{restore_color}}}"),
        None => format!("{{{restore_bold}}}"),
    };
    let mut result = String::new();
    let mut cursor = 0;
    for (begin, finish) in merged {
        result.push_str(&escape_ass(&text[cursor..begin]));
        result.push_str(&start_tag);
        result.push_str(&escape_ass(&text[begin..finish]));
        result.push_str(&end_tag);
        cursor = finish;
    }
    result.push_str(&escape_ass(&text[cursor..]));
    result
}

fn fade_milliseconds(start: f64, end: f64) -> u64 {
    (((end - start) * 250.0).floor() as u64).min(120)
}

fn timestamp(value: f64) -> String {
    let centiseconds = (value * 100.0).round() as u64;
    format!(
        "{}:{:02}:{:02}.{:02}",
        centiseconds / 360000,
        (centiseconds / 6000) % 60,
        (centiseconds / 100) % 60,
        centiseconds % 100
    )
}

fn escape_ass(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('\n', "\\N")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn emphasizes_keywords_with_gold_bold() {
        let text = emphasize(
            "两万多预算的柴油皮卡现车",
            &["柴油皮卡"],
            Some(GOLD),
            None,
            true,
        );
        assert!(text.contains("{\\b1\\c&H2CA6D6&}柴油皮卡{\\b1\\c&HFFFFFF&}现车"));
    }

    #[test]
    fn russian_lines_bold_only() {
        let text = emphasize(
            "Дизельный пикап надежный",
            &["пикап"],
            None,
            Some(RU_YELLOW),
            false,
        );
        assert!(text.contains("{\\b1}пикап{\\b0} надежный"));
        assert!(!text.contains(GOLD));
    }

    #[test]
    fn merges_overlapping_keyword_ranges() {
        let text = emphasize("柴油皮卡", &["皮卡", "柴油皮卡"], Some(GOLD), None, true);
        assert!(text.contains("{\\b1\\c&H2CA6D6&}柴油皮卡{\\b1\\c&HFFFFFF&}"));
    }

    #[test]
    fn events_on_timeline_are_written_with_fade() {
        let edl =
            json!({"video_segments":[{"in":5.0,"out":15.0,"timeline_in":0.0,"timeline_out":10.0}]});
        let events = vec![json!({
            "start": 1.0, "end": 2.5,
            "zh": "你好", "en": "Hello",
            "keywords": {"zh": ["你好"], "en": []}
        })];
        let body = ass(&edl, &events, "zh", "en").expect("ass");
        assert!(body.contains("Dialogue: 0,0:00:01.00,0:00:02.50"));
        assert!(body.contains("\\fad(120,120)"));
    }

    #[test]
    fn short_events_keep_an_opaque_interval() {
        let edl =
            json!({"video_segments":[{"in":0.0,"out":1.0,"timeline_in":0.0,"timeline_out":1.0}]});
        let events = vec![json!({
            "start": 0.0, "end": 0.2,
            "zh": "好", "en": "Good",
            "keywords": {"zh": [], "en": []}
        })];

        let body = ass(&edl, &events, "zh", "en").expect("render short caption");
        assert!(body.contains("\\fad(50,50)"));
    }

    #[test]
    fn rejects_events_beyond_timeline() {
        let edl =
            json!({"video_segments":[{"in":0.0,"out":5.0,"timeline_in":0.0,"timeline_out":5.0}]});
        let events = vec![json!({"start": 6.0, "end": 8.0, "zh": "超界", "en": "out"})];
        assert!(ass(&edl, &events, "zh", "en").is_err());
    }
}
