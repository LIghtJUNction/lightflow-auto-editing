use std::path::Path;
use std::process::Command;

use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};

use crate::RuntimeError;
use crate::media::{artifact_path, command, input_text, output_path, probe, source_path, text};

pub(crate) fn execute(inputs: &Map<String, Value>, base: &Path) -> Result<Response, RuntimeError> {
    let source = source_path(base, input_text(inputs, "source_path")?, "source_path")?;
    let video_output = output_path(base, input_text(inputs, "output_path")?, "output_path")?;
    let srt_output = output_path(
        base,
        input_text(inputs, "srt_output_path")?,
        "srt_output_path",
    )?;
    let font = source_path(base, input_text(inputs, "font_path")?, "font_path")?;
    if video_output.extension().and_then(|value| value.to_str()) != Some("mp4")
        || srt_output.extension().and_then(|value| value.to_str()) != Some("srt")
    {
        return Err(RuntimeError::new("subtitle outputs must be .mp4 and .srt"));
    }
    let (duration, _) = probe(&source)?;
    let language = input_text(inputs, "selected_language")?;
    let tracks = inputs
        .get("subtitle_tracks")
        .and_then(Value::as_array)
        .ok_or_else(|| RuntimeError::new("subtitle_tracks must be an array"))?;
    let track = tracks
        .iter()
        .find(|track| track.get("language").and_then(Value::as_str) == Some(language))
        .ok_or_else(|| RuntimeError::new("selected_language is not present in subtitle_tracks"))?;
    let cues = track
        .get("cues")
        .and_then(Value::as_array)
        .filter(|cues| !cues.is_empty())
        .ok_or_else(|| RuntimeError::new("selected subtitle track has no cues"))?;
    let mut srt = String::new();
    let mut previous_end = 0_u64;
    for (index, cue) in cues.iter().enumerate() {
        let start = cue
            .get("start_ms")
            .and_then(Value::as_u64)
            .ok_or_else(|| RuntimeError::new("cue.start_ms must be an integer"))?;
        let end = cue
            .get("end_ms")
            .and_then(Value::as_u64)
            .ok_or_else(|| RuntimeError::new("cue.end_ms must be an integer"))?;
        if start < previous_end || end <= start || end as f64 > duration * 1000.0 + 1.0 {
            return Err(RuntimeError::new(
                "subtitle cues must be ordered, non-overlapping, and within source duration",
            ));
        }
        previous_end = end;
        let body = text(
            cue.get("text")
                .ok_or_else(|| RuntimeError::new("cue.text missing"))?,
            "cue.text",
        )?;
        srt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            index + 1,
            srt_time(start),
            srt_time(end),
            body.replace('\r', "")
        ));
    }
    std::fs::write(&srt_output, srt).map_err(RuntimeError::io)?;
    let srt_filter = escape_filter(&srt_output);
    let fonts_dir = font
        .parent()
        .ok_or_else(|| RuntimeError::new("font_path has no parent"))?;
    let mut command_line = Command::new("ffmpeg");
    command_line
        .args(["-y", "-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
        .arg(&source)
        .args([
            "-vf",
            &format!(
                "subtitles=filename='{srt_filter}':fontsdir='{}'",
                escape_filter(fonts_dir)
            ),
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-c:a",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(&video_output);
    command(&mut command_line, "FFmpeg subtitle burn-in")?;
    let video = json!({"id":"video","kind":"video","path":artifact_path(base, &video_output)?,"mime_type":"video/mp4","metadata":{"language":language,"cue_count":cues.len()}});
    let subtitles = json!({"id":"subtitles","kind":"text","path":artifact_path(base, &srt_output)?,"mime_type":"application/x-subrip","metadata":{"language":language,"cue_count":cues.len()}});
    let summary = format!(
        "Burned {} {language} subtitle cues into {}.",
        cues.len(),
        video_output.display()
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("video".to_owned(), video),
            (
                "video_path".to_owned(),
                artifact_path(base, &video_output)?.into(),
            ),
            ("subtitles".to_owned(), subtitles),
            (
                "subtitles_path".to_owned(),
                artifact_path(base, &srt_output)?.into(),
            ),
            ("render_summary".to_owned(), summary.into()),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::new(),
    })
}

fn srt_time(milliseconds: u64) -> String {
    let hours = milliseconds / 3_600_000;
    let minutes = (milliseconds / 60_000) % 60;
    let seconds = (milliseconds / 1_000) % 60;
    format!(
        "{hours:02}:{minutes:02}:{seconds:02},{:03}",
        milliseconds % 1_000
    )
}

fn escape_filter(path: &Path) -> String {
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
    fn formats_srt_timestamp() {
        assert_eq!(srt_time(3_661_002), "01:01:01,002");
    }
}
