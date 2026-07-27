use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::{Map, Value, json};

mod audit;
mod cover_references;
mod cover_render;
mod covers;
mod delivery;
mod subtitles;
use audit::{audit_output, timeline_duration};
use covers::render_covers;
use delivery::package;
use subtitles::{ass, burn};

const ROOT: &str = "/srv";
const TASK_PREFIXES: [&str; 2] = ["批量剪辑/", "精剪/"];

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(value) => match serde_json::to_string(&value) {
            Ok(text) => {
                println!("{text}");
                ExitCode::SUCCESS
            }
            Err(error) => fail(format!("cannot encode response: {error}")),
        },
        Err(error) => fail(error),
    }
}

fn fail(message: String) -> ExitCode {
    eprintln!("lightflow-xry-worker: {message}");
    ExitCode::from(2)
}

fn run(args: Vec<String>) -> Result<Value, String> {
    let (action, task, subject) = parse_args(&args)?;
    let production = production_path(&task, subject.as_deref())?;
    match action.as_str() {
        "status" => status(&task, subject.as_deref(), &production),
        "reject" => reject(&task, subject.as_deref(), &production),
        "audit" => audit_only(
            &task,
            subject.as_deref().ok_or("audit requires --subject")?,
            &production,
        ),
        "package" => package(
            &task,
            subject.as_deref().ok_or("package requires --subject")?,
            &production,
        ),
        "cover" => render_cover_only(
            &task,
            subject.as_deref().ok_or("cover requires --subject")?,
            &production,
        ),
        "produce" => render(
            &task,
            subject.as_deref().ok_or("produce requires --subject")?,
            &production,
        ),
        _ => Err("action must be status, reject, audit, cover, produce, or package".to_owned()),
    }
}

fn parse_args(args: &[String]) -> Result<(String, String, Option<String>), String> {
    let action = args.first().ok_or("missing action")?.to_owned();
    let mut task = None;
    let mut subject = None;
    let mut index = 1;
    while index < args.len() {
        let value = args.get(index + 1).ok_or("missing flag value")?.to_owned();
        match args[index].as_str() {
            "--task" => task = Some(value),
            "--subject" => subject = Some(value),
            _ => return Err(format!("unsupported argument {}", args[index])),
        }
        index += 2;
    }
    let task = task.ok_or("missing --task")?;
    if !TASK_PREFIXES.iter().any(|prefix| task.starts_with(prefix))
        || task.split('/').any(|part| part.is_empty() || part == "..")
    {
        return Err("task must be a safe path below 批量剪辑/ or 精剪/".to_owned());
    }
    if subject
        .as_deref()
        .is_some_and(|value| !valid_subject(value))
    {
        return Err("subject must be an S-number".to_owned());
    }
    Ok((action, task, subject))
}

fn valid_subject(value: &str) -> bool {
    value.strip_prefix('S').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn production_path(task: &str, subject: Option<&str>) -> Result<PathBuf, String> {
    let task_dir = Path::new(ROOT).join("2.预处理").join(task);
    let production = match subject {
        Some(value) => task_dir.join(".pipeline/production").join(value),
        None => task_dir.join(".pipeline"),
    };
    if !production.starts_with(Path::new(ROOT)) {
        return Err("resolved production path is unsafe".to_owned());
    }
    Ok(production)
}

fn status(task: &str, subject: Option<&str>, path: &Path) -> Result<Value, String> {
    let mut checks = Map::new();
    for name in [
        "edl.json",
        "captions.json",
        "rendered.master.mp4",
        "rendered.ze.mp4",
        "rendered.re.mp4",
        "subs.zh-en.ass",
        "subs.ru-en.ass",
        "quality-gate.json",
    ] {
        checks.insert(name.to_owned(), path.join(name).is_file().into());
    }
    Ok(
        json!({"ok":true,"action":"status","task":task,"subject":subject,"production":path,"checks":checks}),
    )
}

fn reject(task: &str, subject: Option<&str>, path: &Path) -> Result<Value, String> {
    let subject = subject.ok_or("reject requires --subject")?;
    let edl = path.join("edl.json");
    if !edl.is_file() {
        return Err("cannot reject a subject without edl.json".to_owned());
    }
    let gate = json!({"schema_version":1,"status":"REJECTED","reason":"requires_rerender_after_editorial_or_caption_change","task":task,"subject":subject,"edl_bytes":fs::metadata(&edl).map_err(|error| error.to_string())?.len()});
    atomic_json(&path.join("quality-gate.json"), &gate)?;
    Ok(gate)
}

fn render_cover_only(task: &str, subject: &str, production: &Path) -> Result<Value, String> {
    let edl = read_json(&production.join("edl.json"))?;
    let captions = read_json(&production.join("captions.json"))?;
    render_covers(
        &Path::new(ROOT).join("1.素材").join(task),
        production,
        &edl,
        &captions,
    )?;
    Ok(
        json!({"ok":true,"action":"cover","task":task,"subject":subject,"cover_ze":production.join("cover.ze.jpg"),"cover_re":production.join("cover.re.jpg")}),
    )
}

fn audit_only(task: &str, subject: &str, production: &Path) -> Result<Value, String> {
    let edl = read_json(&production.join("edl.json"))?;
    let gate_path = production.join("quality-gate.json");
    match audit_output(
        production,
        &edl,
        &production.join("subs.zh-en.ass"),
        &production.join("subs.ru-en.ass"),
    ) {
        Ok(audit) => {
            let gate = json!({"schema_version":1,"status":"PASS","task":task,"subject":subject,"audit":audit,"audited_by":"lightflow-xry-worker.rust"});
            atomic_json(&gate_path, &gate)?;
            Ok(gate)
        }
        Err(reason) => {
            let gate = json!({"schema_version":1,"status":"REJECTED","task":task,"subject":subject,"reason":reason,"audited_by":"lightflow-xry-worker.rust"});
            atomic_json(&gate_path, &gate)?;
            Err(format!("quality audit rejected {subject}: {reason}"))
        }
    }
}

fn render(task: &str, subject: &str, production: &Path) -> Result<Value, String> {
    let gate_path = production.join("quality-gate.json");
    if gate_path.is_file() {
        let gate = read_json(&gate_path)?;
        if gate.get("status").and_then(Value::as_str) != Some("REJECTED") {
            return Err("production requires a rejected quality-gate before rerender".to_owned());
        }
    }
    let edl = read_json(&production.join("edl.json"))?;
    let segments = edl
        .get("video_segments")
        .and_then(Value::as_array)
        .filter(|value| !value.is_empty())
        .ok_or("edl.video_segments must be non-empty")?;
    let source_root = Path::new(ROOT).join("1.素材").join(task);
    let master = production.join("rendered.master.mp4");
    let mut command = Command::new("ffmpeg");
    command.args(["-y", "-hide_banner", "-nostdin", "-loglevel", "error"]);
    let mut filters = Vec::new();
    let mut labels = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let item = segment.as_object().ok_or("edl segment must be an object")?;
        let source_name = field_text(item, "source")?;
        if Path::new(source_name)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(source_name)
        {
            return Err("edl source must be a basename".to_owned());
        }
        let source = source_root.join(source_name);
        if !source.is_file() {
            return Err(format!("source is missing: {}", source.display()));
        }
        let start = field_number(item, "in")?;
        let end = field_number(item, "out")?;
        if !(0.0 <= start && start < end) {
            return Err("edl segment bounds are invalid".to_owned());
        }
        command.args([
            "-ss",
            &format!("{start:.6}"),
            "-t",
            &format!("{:.6}", end - start),
            "-i",
        ]);
        command.arg(source);
        filters.push(format!("[{index}:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30,format=yuv420p[v{index}]"));
        filters.push(format!(
            "[{index}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{index}]"
        ));
        labels.push(format!("[v{index}][a{index}]"));
    }
    filters.push(format!(
        "{}concat=n={}:v=1:a=1[outv][outa]",
        labels.join(""),
        segments.len()
    ));
    let mut video_label = "outv".to_owned();
    let overlays = edl
        .get("b_roll_overlays")
        .and_then(Value::as_array)
        .ok_or("edl.b_roll_overlays must be an array")?;
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let item = overlay
            .as_object()
            .ok_or("b-roll overlay must be an object")?;
        let declared = item.get("type").and_then(Value::as_str);
        let kind = match declared {
            Some("hold") => "hold",
            Some("clip") => "clip",
            None if item.contains_key("source_time") => "hold",
            None if item.contains_key("in") && item.contains_key("out") => "clip",
            _ => return Err("b-roll overlay type must be hold or clip".to_owned()),
        };
        let source_name = field_text(item, "source")?;
        if Path::new(source_name)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(source_name)
        {
            return Err("b-roll source must be a basename".to_owned());
        }
        let source = source_root.join(source_name);
        if !source.is_file() {
            return Err(format!("b-roll source is missing: {}", source.display()));
        }
        let timeline_in = field_number(item, "timeline_in")?;
        let timeline_out = field_number(item, "timeline_out")?;
        if !(0.0 <= timeline_in && timeline_in < timeline_out) {
            return Err("b-roll overlay timing is invalid".to_owned());
        }
        let window = timeline_out - timeline_in;
        let seek = if kind == "hold" {
            let source_time = field_number(item, "source_time")?;
            if source_time < 0.0 {
                return Err("b-roll overlay timing is invalid".to_owned());
            }
            source_time
        } else {
            let clip_in = field_number(item, "in")?;
            let clip_out = field_number(item, "out")?;
            if !(0.0 <= clip_in && clip_in < clip_out) {
                return Err("b-roll overlay timing is invalid".to_owned());
            }
            if ((clip_out - clip_in) - window).abs() > 0.05 {
                return Err("clip b-roll duration must match its timeline window".to_owned());
            }
            clip_in
        };
        let input_index = segments.len() + overlay_index;
        command.args([
            "-ss",
            &format!("{seek:.6}"),
            "-t",
            &format!("{window:.6}"),
            "-i",
        ]);
        command.arg(source);
        let overlay_label = format!("overlay{overlay_index}");
        let result_label = format!("main{overlay_index}");
        // A clip overlay plays through its window, so its frames must carry
        // timeline PTS; a hold shows one cloned frame for the whole window.
        let pts = if kind == "clip" {
            format!(",setpts=PTS-STARTPTS+{timeline_in:.6}/TB")
        } else {
            String::new()
        };
        filters.push(format!("[{input_index}:v]scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30,format=yuv420p{pts}[{overlay_label}]"));
        filters.push(format!("[{video_label}][{overlay_label}]overlay=enable='between(t,{timeline_in:.6},{timeline_out:.6})'[{result_label}]"));
        video_label = result_label;
    }
    command.args([
        "-filter_complex",
        &filters.join(";"),
        "-map",
        &format!("[{video_label}]"),
        "-map",
        "[outa]",
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "20",
        "-c:a",
        "aac",
        "-b:a",
        "160k",
        "-movflags",
        "+faststart",
    ]);
    command.arg(&master);
    run_command(&mut command, "render master")?;
    let captions = read_json(&production.join("captions.json"))?;
    let events = captions
        .get("events")
        .and_then(Value::as_array)
        .ok_or("captions.events must be an array")?;
    let ze_ass = ass(&edl, events, "zh", "en")?;
    let re_ass = ass(&edl, events, "ru", "en")?;
    let ze_file = production.join("subs.zh-en.ass");
    let re_file = production.join("subs.ru-en.ass");
    fs::write(&ze_file, ze_ass).map_err(|error| error.to_string())?;
    fs::write(&re_file, re_ass).map_err(|error| error.to_string())?;
    burn(&master, &ze_file, &production.join("rendered.ze.mp4"))?;
    burn(&master, &re_file, &production.join("rendered.re.mp4"))?;
    render_covers(&source_root, production, &edl, &captions)?;
    let audit = audit_output(production, &edl, &ze_file, &re_file)?;
    let gate = json!({"schema_version":1,"status":"PASS","task":task,"subject":subject,"edl_duration_seconds":timeline_duration(&edl)?,"variants":{"ZE":{"main":"zh","sub":"en"},"RE":{"main":"ru","sub":"en"}},"audit":audit,"rendered_by":"lightflow-xry-worker.rust"});
    atomic_json(&gate_path, &gate)?;
    Ok(gate)
}

fn is_cjk(character: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&character)
}
fn is_cyrillic(character: char) -> bool {
    ('\u{0400}'..='\u{052f}').contains(&character)
}

fn field_text<'a>(object: &'a Map<String, Value>, name: &str) -> Result<&'a str, String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing text field {name}"))
}
fn field_number(object: &Map<String, Value>, name: &str) -> Result<f64, String> {
    object
        .get(name)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("missing number field {name}"))
}
fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("cannot parse {}: {error}", path.display()))
}
fn atomic_json(path: &Path, value: &Value) -> Result<(), String> {
    let temporary = path.with_extension("json.lightflow-tmp");
    fs::write(
        &temporary,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
        ),
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}
fn run_command(command: &mut Command, label: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{label} failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests;
