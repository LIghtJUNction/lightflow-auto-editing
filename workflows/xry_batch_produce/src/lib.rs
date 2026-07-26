use std::process::Command;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.xry_batch_produce";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

const STAGES: &[&str] = &[
    "ensure-ids",
    "ensure-publication",
    "subtitles",
    "hook-evidence",
    "pre-render",
    "render",
    "cover",
    "package",
    "pre-package",
    "task-state",
];

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "XRY Batch Produce",
        description: "Rust-native LightFlow boundary for canonical XRY production.",
        input "task": "text" {
            description: "Existing task relative to /srv/xry/2.预处理/.",
            required: true,
            widget: "text",
        }
        input "subject": "text" {
            description: "Subject ID such as S01.",
            required: true,
            widget: "text",
        }
        input "commit_package": "boolean" {
            description: "Allows validated package commit.",
            required: false,
            default: false,
            widget: "checkbox",
        }
        input "from_stage": "text" {
            description: "Optional canonical resume stage.",
            required: false,
            widget: "select",
            choices: ["ensure-ids", "ensure-publication", "subtitles", "hook-evidence", "pre-render", "render", "cover", "package", "pre-package", "task-state"],
        }
        input "to_stage": "text" {
            description: "Optional canonical end stage.",
            required: false,
            widget: "select",
            choices: ["ensure-ids", "ensure-publication", "subtitles", "hook-evidence", "pre-render", "render", "cover", "package", "pre-package", "task-state"],
        }
        output "production_report": "json" { description: "Canonical production report." }
        output "task_state_path": "path" { description: "Canonical task-state snapshot." }
        output "summary": "text" { description: "Rust-native production summary." }
    }
    .builtin_runtime("command", "lightflow.command.run", "runner.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, ProduceError> {
    let task = safe_task(required_text(inputs, "task")?)?;
    let subject = safe_subject(required_text(inputs, "subject")?)?;
    let commit = inputs
        .get("commit_package")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let from = optional_stage(inputs, "from_stage")?;
    let to = optional_stage(inputs, "to_stage")?;
    if let (Some(from), Some(to)) = (from, to)
        && stage_index(from) > stage_index(to)
    {
        return Err(ProduceError::new("from_stage must not follow to_stage"));
    }
    if commit {
        return Err(ProduceError::new(
            "package commit is intentionally unavailable until the Rust delivery audit is implemented",
        ));
    }
    if from.is_some() || to.is_some() {
        return Err(ProduceError::new(
            "stage slicing is unavailable in the Rust worker; it always rerenders the full rejected subject",
        ));
    }
    let command = vec![
        "/srv/.lightflow/bin/lightflow-xry-worker",
        "produce",
        "--task",
        &task,
        "--subject",
        &subject,
    ];
    let output = Command::new("ssh")
        .args([
            "-F",
            "/home/lightjunction/.ssh/config",
            "-o",
            "ControlMaster=no",
            "-o",
            "ControlPath=none",
            "-o",
            "ControlPersist=no",
            "xry",
        ])
        .args(command)
        .output()
        .map_err(|error| ProduceError::owned(format!("cannot start XRY production: {error}")))?;
    if !output.status.success() {
        return Err(ProduceError::owned(format!(
            "XRY production failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let report: Value = lightflow::serde_json::from_slice(&output.stdout)
        .map_err(|error| ProduceError::owned(format!("XRY returned invalid JSON: {error}")))?;
    Ok(Response {
        outputs: Map::from_iter([
            ("production_report".to_owned(), report),
            (
                "task_state_path".to_owned(),
                format!("/srv/2.预处理/{task}/.pipeline/task-state.json").into(),
            ),
            (
                "summary".to_owned(),
                "XRY canonical production completed through Rust-native LightFlow.".into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([
            (
                "implementation".to_owned(),
                implementation_identity().into(),
            ),
            ("task".to_owned(), task.into()),
            ("subject".to_owned(), subject.into()),
        ]),
    })
}

fn required_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<&'a str, ProduceError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .ok_or_else(|| ProduceError::new("missing required text input"))
}
fn safe_task(value: &str) -> Result<String, ProduceError> {
    if !value.starts_with("批量剪辑/")
        || value.split('/').any(|part| part.is_empty() || part == "..")
    {
        return Err(ProduceError::new(
            "task must be a safe path below 批量剪辑/",
        ));
    }
    Ok(value.to_owned())
}
fn safe_subject(value: &str) -> Result<String, ProduceError> {
    if !value.starts_with('S')
        || !value[1..]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(ProduceError::new("subject must be an S-number"));
    }
    Ok(value.to_owned())
}
fn optional_stage<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<Option<&'a str>, ProduceError> {
    match inputs.get(name) {
        None => Ok(None),
        Some(Value::String(value)) if STAGES.contains(&value.as_str()) => Ok(Some(value.as_str())),
        Some(_) => Err(ProduceError::new("stage must be canonical")),
    }
}
fn stage_index(stage: &str) -> usize {
    STAGES
        .iter()
        .position(|candidate| *candidate == stage)
        .expect("validated stage")
}
fn implementation_identity() -> String {
    format!(
        "lightflow.xry_batch_produce.rust.fnv1a64:{:016x}",
        digest(include_bytes!("lib.rs"))
    )
}
const fn digest(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
        index += 1;
    }
    hash
}
#[derive(Debug)]
pub struct ProduceError(String);
impl ProduceError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn owned(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for ProduceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for ProduceError {}
