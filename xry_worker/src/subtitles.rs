use super::{field_number, field_text, run_command};
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

pub(super) fn ass(edl: &Value, events: &[Value], main: &str, sub: &str) -> Result<String, String> {
    let segments = edl
        .get("video_segments")
        .and_then(Value::as_array)
        .ok_or("edl.video_segments missing")?;
    let mut output = String::from(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920\n\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\nStyle: Main,Noto Sans CJK SC,92,&H00FFFFFF,&H000000FF,&H00101010,&H70000000,-1,0,0,0,100,100,0,0,1,4,1,2,60,60,190,1\nStyle: Sub,Noto Sans,54,&H00FFFFFF,&H000000FF,&H00101010,&H70000000,0,0,0,0,100,100,0,0,1,3,1,2,60,60,105,1\n\n[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\n",
    );
    for event in events {
        let item = event.as_object().ok_or("caption event must be object")?;
        let start = field_number(item, "start")?;
        let end = field_number(item, "end")?;
        let primary = field_text(item, main)?;
        let secondary = field_text(item, sub)?;
        for segment in segments {
            let segment = segment.as_object().ok_or("edl segment must be object")?;
            let in_time = field_number(segment, "in")?;
            let out_time = field_number(segment, "out")?;
            let timeline_in = field_number(segment, "timeline_in")?;
            let left = start.max(in_time);
            let right = end.min(out_time);
            if right > left {
                let rendered_start = timeline_in + left - in_time;
                let rendered_end = timeline_in + right - in_time;
                output.push_str(&format!(
                    "Dialogue: 0,{}, {},Main,,0,0,0,,{}\\N{{\\rSub}}{}\n",
                    timestamp(rendered_start),
                    timestamp(rendered_end),
                    escape_ass(primary),
                    escape_ass(secondary)
                ));
            }
        }
    }
    Ok(output)
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
