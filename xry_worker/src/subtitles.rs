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
    let duration = super::timeline_duration(edl)?;
    let mut output = String::from(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 1080\nPlayResY: 1920\n\n[V4+ Styles]\nFormat: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\nStyle: Main,Noto Sans CJK SC,92,&H00FFFFFF,&H000000FF,&H00101010,&H70000000,-1,0,0,0,100,100,0,0,1,4,1,2,60,60,190,1\nStyle: Sub,Noto Sans,54,&H00FFFFFF,&H000000FF,&H00101010,&H70000000,0,0,0,0,100,100,0,0,1,3,1,2,60,60,105,1\n\n[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\n",
    );
    // Caption events are authored on the OUTPUT timeline (canonical semantics);
    // they must be ascending, non-overlapping, and inside the frozen duration.
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
        output.push_str(&format!(
            "Dialogue: 0,{},{},Main,,0,0,0,,{}\\N{{\\rSub}}{}\n",
            timestamp(start.max(0.0)),
            timestamp(end.min(duration)),
            escape_ass(primary),
            escape_ass(secondary)
        ));
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
