//! Public, fail-closed control entrypoint for one frozen XRY task subject.

use std::fmt;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};
use lightflow_xry_gateway::{ControlAction, GatewayError, GatewayRequest, invoke};

pub const WORKFLOW_ID: &str = "lightflow.xry_batch_control";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

const INPUTS: &[&str] = &["action", "task", "subject", "apply", "plan_sha256"];
const IMPLEMENTATION: &str = concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION"));

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "XRY Batch Control",
        description: "Run one canonical, bound XRY control action through the locked gateway.",
        input "action": "text" {
            description: "Bound control action: progress, freeze, cleanup, or archive.",
            required: true,
            choices: ["progress", "freeze", "cleanup", "archive"],
            widget: "select",
        }
        input "task": "text" {
            description: "Exact frozen task binding: 批量剪辑/<group>/<batch>.",
            required: true,
            widget: "text",
        }
        input "subject": "text" {
            description: "Exact frozen subject binding, for example S01.",
            required: true,
            widget: "text",
        }
        input "apply": "boolean" {
            description: "False performs a dry run. True is accepted only for cleanup or archive with a confirmed exact plan SHA-256.",
            required: false,
            default: false,
            widget: "toggle",
        }
        input "plan_sha256": "text" {
            description: "Exact confirmed cleanup or archive plan SHA-256 required when apply is true.",
            required: false,
            widget: "text",
        }
        output "report": "json" {
            description: "Opaque canonical report returned by a verified gateway PASS response.",
        }
        output "summary": "text" {
            description: "Verified canonical control outcome summary.",
        }
    }
    .builtin_runtime("runner", "lightflow.runner", "runner.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, ControlError> {
    let request = request_from_inputs(inputs)?;
    let action = request.action();
    let response = invoke(&request).map_err(ControlError::from)?;
    let replay_fingerprint = with_package_implementation(
        response
            .replay_fingerprint()
            .as_object()
            .cloned()
            .ok_or_else(|| ControlError::new("gateway replay fingerprint must be an object"))?,
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("report".to_owned(), response.report().clone()),
            (
                "summary".to_owned(),
                format!(
                    "Canonical {} PASS was verified for the bound XRY task subject.",
                    action.as_str()
                )
                .into(),
            ),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint,
    })
}

fn with_package_implementation(mut fingerprint: Map<String, Value>) -> Map<String, Value> {
    fingerprint.insert("implementation".to_owned(), IMPLEMENTATION.into());
    fingerprint
}

fn request_from_inputs(inputs: &Map<String, Value>) -> Result<GatewayRequest, ControlError> {
    reject_unknown_inputs(inputs)?;
    let action =
        ControlAction::parse(required_text(inputs, "action")?).map_err(ControlError::from)?;
    let task = required_text(inputs, "task")?;
    let subject = required_text(inputs, "subject")?;
    let apply = optional_boolean(inputs, "apply", false)?;
    let plan_sha256 = optional_text(inputs, "plan_sha256")?;
    GatewayRequest::control(action, task, subject, apply, plan_sha256).map_err(ControlError::from)
}

fn reject_unknown_inputs(inputs: &Map<String, Value>) -> Result<(), ControlError> {
    if inputs.keys().all(|name| INPUTS.contains(&name.as_str())) {
        Ok(())
    } else {
        Err(ControlError::new(
            "control request contains an unsupported input",
        ))
    }
}

fn required_text<'a>(inputs: &'a Map<String, Value>, name: &str) -> Result<&'a str, ControlError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ControlError::new("control request has a missing or invalid required text input")
        })
}

fn optional_text(inputs: &Map<String, Value>, name: &str) -> Result<Option<String>, ControlError> {
    match inputs.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        _ => Err(ControlError::new(
            "control request has an invalid optional text input",
        )),
    }
}

fn optional_boolean(
    inputs: &Map<String, Value>,
    name: &str,
    default: bool,
) -> Result<bool, ControlError> {
    match inputs.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(ControlError::new(
            "control request has an invalid optional boolean input",
        )),
    }
}

#[derive(Debug)]
pub struct ControlError(String);

impl ControlError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<GatewayError> for ControlError {
    fn from(error: GatewayError) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ControlError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::json;

    fn valid_inputs() -> Map<String, Value> {
        Map::from_iter([
            ("action".to_owned(), json!("progress")),
            (
                "task".to_owned(),
                json!("批量剪辑/皮卡严选 走全球/7.23批量"),
            ),
            ("subject".to_owned(), json!("S01")),
        ])
    }

    #[test]
    fn definition_uses_the_package_identity_and_fixed_actions() {
        let definition = define();
        assert_eq!(definition.id, WORKFLOW_ID);
        assert_eq!(definition.version, WORKFLOW_VERSION);
        assert_eq!(definition.inputs.len(), INPUTS.len());
        assert_eq!(definition.runtimes.len(), 1);
        assert_eq!(definition.runtimes[0].id, "runner");
        assert_eq!(definition.runtimes[0].capability, "lightflow.runner");
        assert_eq!(definition.runtimes[0].engine.as_deref(), Some("runner.v1"));
    }

    #[test]
    fn replay_fingerprint_identifies_this_package_runner() {
        let fingerprint = with_package_implementation(Map::new());
        assert_eq!(
            fingerprint.get("implementation"),
            Some(&Value::String(IMPLEMENTATION.to_owned()))
        );
    }

    #[test]
    fn request_rejects_unbound_or_legacy_inputs() {
        assert!(request_from_inputs(&valid_inputs()).is_ok());

        let mut missing_subject = valid_inputs();
        missing_subject.remove("subject");
        assert!(request_from_inputs(&missing_subject).is_err());

        let mut legacy = valid_inputs();
        legacy.insert("commit_package".to_owned(), json!(true));
        assert!(request_from_inputs(&legacy).is_err());
    }

    #[test]
    fn cleanup_apply_needs_a_confirmed_plan_hash() {
        let mut request = valid_inputs();
        request.insert("action".to_owned(), json!("cleanup"));
        request.insert("apply".to_owned(), json!(true));
        assert!(request_from_inputs(&request).is_err());
        request.insert(
            "plan_sha256".to_owned(),
            json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        assert!(request_from_inputs(&request).is_ok());
    }
}
