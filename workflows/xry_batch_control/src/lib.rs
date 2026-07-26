use std::process::Command;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};

pub const WORKFLOW_ID: &str = "lightflow.xry_batch_control";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "XRY Batch Control",
        description: "Rust-native, allow-listed LightFlow control for XRY batch operations.",
        input "action": "text" {
            description: "One of status, reject, audit, cover, produce, or package.",
            required: true,
            widget: "select",
            choices: ["status", "reject", "audit", "cover", "produce", "package"],
        }
        input "task": "text" {
        description: "Existing task relative to /srv/2.预处理/.",
            required: true,
            widget: "text",
        }
        input "subject": "text" {
            description: "Required for reject, audit, cover, produce, and package.",
            required: false,
            widget: "text",
        }
        output "report": "json" { description: "Canonical XRY report." }
        output "summary": "text" { description: "Rust-native LightFlow control summary." }
    }
    .builtin_runtime("command", "lightflow.command.run", "runner.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, ControlError> {
    let action = required_text(inputs, "action")?;
    let task = safe_task(required_text(inputs, "task")?)?;
    let subject = optional_text(inputs, "subject")?;
    let command = canonical_command(action, &task, subject)?;
    let output = Command::new("ssh")
        .args(ssh_arguments(command))
        .output()
        .map_err(|error| ControlError::owned(format!("cannot start XRY control: {error}")))?;
    if !output.status.success() {
        return Err(ControlError::owned(format!(
            "XRY control failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| ControlError::new("XRY control returned non-UTF-8 output"))?;
    let report = lightflow::serde_json::from_str(&stdout)
        .map_err(|error| ControlError::owned(format!("XRY returned invalid JSON: {error}")))?;
    Ok(Response {
        outputs: Map::from_iter([
            ("report".to_owned(), report),
            (
                "summary".to_owned(),
                format!("XRY {action} completed through Rust-native LightFlow.").into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([
            (
                "implementation".to_owned(),
                implementation_identity().into(),
            ),
            ("task".to_owned(), task.into()),
        ]),
    })
}

fn canonical_command<'a>(
    action: &'a str,
    task: &'a str,
    subject: Option<&'a str>,
) -> Result<String, ControlError> {
    let worker = "/srv/.lightflow/bin/lightflow-xry-worker";
    let arguments = match action {
        "status" => Ok(vec![worker, "status", "--task", task]),
        "reject" | "audit" | "cover" | "produce" | "package" => Ok(vec![
            worker,
            action,
            "--task",
            task,
            "--subject",
            subject.ok_or_else(|| ControlError::new("this action requires subject"))?,
        ]),
        _ => Err(ControlError::new("unsupported XRY control action")),
    }?;
    Ok(remote_shell_command(&arguments))
}

fn ssh_arguments(remote_command: String) -> Vec<String> {
    [
        "-F",
        "/home/lightjunction/.ssh/config",
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "-o",
        "ControlPersist=no",
        "xry",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(std::iter::once(remote_command))
    .collect()
}

fn remote_shell_command(arguments: &[&str]) -> String {
    arguments
        .iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn required_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<&'a str, ControlError> {
    optional_text(inputs, name)?.ok_or_else(|| ControlError::new("missing required input"))
}

fn optional_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &'static str,
) -> Result<Option<&'a str>, ControlError> {
    match inputs.get(name) {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.trim())),
        Some(_) => Err(ControlError::new("text input must be non-empty")),
    }
}

fn safe_task(task: &str) -> Result<String, ControlError> {
    if !(task.starts_with("批量剪辑/") || task.starts_with("精剪/"))
        || task.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err(ControlError::new(
            "task must be a safe path below 批量剪辑/ or 精剪/",
        ));
    }
    Ok(task.to_owned())
}

fn implementation_identity() -> String {
    format!(
        "lightflow.xry_batch_control.rust.fnv1a64:{:016x}",
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
pub struct ControlError(String);
impl ControlError {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    fn owned(value: String) -> Self {
        Self(value)
    }
}
impl std::fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for ControlError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_uses_one_quoted_remote_command_for_space_containing_task() {
        let remote_command = canonical_command("status", "批量剪辑/皮卡严选 走全球/7.23批量", None)
            .expect("valid status command");
        assert_eq!(
            remote_command,
            "'/srv/.lightflow/bin/lightflow-xry-worker' 'status' '--task' '批量剪辑/皮卡严选 走全球/7.23批量'"
        );

        let arguments = ssh_arguments(remote_command.clone());
        assert_eq!(arguments.len(), 10);
        assert_eq!(arguments[8], "xry");
        assert_eq!(arguments[9], remote_command);
    }

    #[test]
    fn apostrophes_are_escaped_inside_one_safe_remote_argument() {
        let task = "批量剪辑/王' ; touch /tmp/injected; echo '/7.23批量";
        let remote_command = canonical_command("status", task, None).expect("valid task");
        assert_eq!(
            remote_command,
            "'/srv/.lightflow/bin/lightflow-xry-worker' 'status' '--task' '批量剪辑/王'\\'' ; touch /tmp/injected; echo '\\''/7.23批量'"
        );
        assert_eq!(ssh_arguments(remote_command.clone())[9], remote_command);
    }

    #[test]
    fn only_allow_listed_actions_are_accepted() {
        let error = canonical_command("shell", "批量剪辑/皮卡严选 走全球/7.23批量", None)
            .expect_err("arbitrary actions must be rejected");
        assert_eq!(error.to_string(), "unsupported XRY control action");
    }
}
