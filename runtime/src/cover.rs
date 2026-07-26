use std::path::Path;
use std::process::Command;

use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value, json};

use crate::RuntimeError;
use crate::media::{artifact_path, command, input_text, number, output_path, probe, source_path};

pub(crate) fn execute(inputs: &Map<String, Value>, base: &Path) -> Result<Response, RuntimeError> {
    let source = source_path(base, input_text(inputs, "source_path")?, "source_path")?;
    let output = output_path(base, input_text(inputs, "output_path")?, "output_path")?;
    let group = input_text(inputs, "account_group")?;
    let style = CoverStyle::for_group(group)?;
    let title = input_text(inputs, "title")?;
    let font = source_path(base, input_text(inputs, "font_path")?, "font_path")?;
    let suffix = output
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(suffix.as_str(), "png" | "jpg" | "jpeg") {
        return Err(RuntimeError::new(
            "output_path must end in .png, .jpg, or .jpeg",
        ));
    }
    let (duration, _) = probe(&source)?;
    let timestamp = number(inputs.get("timestamp_seconds"), "timestamp_seconds")?;
    if !(0.0..duration).contains(&timestamp) {
        return Err(RuntimeError::new(
            "timestamp_seconds is outside the source duration",
        ));
    }
    let title_file = output.with_extension("cover-title.lightflow.txt");
    let staged = output.with_file_name(format!(
        ".{}.lightflow-staged.{}",
        output
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| RuntimeError::new("output_path has no valid filename"))?,
        suffix
    ));
    std::fs::write(&title_file, title).map_err(RuntimeError::io)?;
    let filter = format!(
        "drawbox=x={}:y={}:w={}:h={}:color={}:t=fill,drawtext=fontfile='{}':textfile='{}':fontcolor=white:fontsize={}:x={}:y={}",
        style.box_x,
        style.box_y,
        style.box_width,
        style.box_height,
        style.box_color,
        escape_filter(&font),
        escape_filter(&title_file),
        style.font_size,
        style.text_x,
        style.text_y,
    );
    let mut command_line = Command::new("ffmpeg");
    command_line
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
        .arg(&source)
        .args(["-vf", &filter, "-frames:v", "1"])
        .arg(&staged);
    let result = command(&mut command_line, "FFmpeg cover composition");
    let _ = std::fs::remove_file(&title_file);
    result?;
    std::fs::rename(&staged, &output).map_err(RuntimeError::io)?;
    let artifact = json!({"id":"cover","kind":"image","path":artifact_path(base,&output)?,"mime_type":if suffix == "png" {"image/png"} else {"image/jpeg"},"metadata":{"source_path":artifact_path(base,&source)?,"timestamp_seconds":timestamp,"account_group":group,"style":style.name,"bytes":std::fs::metadata(&output).map_err(RuntimeError::io)?.len()}});
    let summary = format!(
        "Composed a {} cover from a source frame into {}.",
        style.name,
        output.display()
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("cover".to_owned(), artifact),
            (
                "cover_path".to_owned(),
                artifact_path(base, &output)?.into(),
            ),
            ("summary".to_owned(), summary.into()),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::new(),
    })
}

struct CoverStyle {
    name: &'static str,
    box_x: &'static str,
    box_y: &'static str,
    box_width: &'static str,
    box_height: &'static str,
    box_color: &'static str,
    font_size: u32,
    text_x: &'static str,
    text_y: &'static str,
}
impl CoverStyle {
    fn for_group(group: &str) -> Result<Self, RuntimeError> {
        match group {
            "zh" => Ok(Self {
                name: "warm-deal-card",
                box_x: "48",
                box_y: "ih-330",
                box_width: "iw-96",
                box_height: "210",
                box_color: "0xE15D22@0.94",
                font_size: 58,
                text_x: "92",
                text_y: "h-270",
            }),
            "overseas" => Ok(Self {
                name: "blue-cyan-export-card",
                box_x: "48",
                box_y: "100",
                box_width: "iw-96",
                box_height: "200",
                box_color: "0x1264D6@0.94",
                font_size: 50,
                text_x: "92",
                text_y: "160",
            }),
            _ => Err(RuntimeError::new("account_group must be zh or overseas")),
        }
    }
}
fn escape_filter(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
        .replace(',', "\\,")
}
