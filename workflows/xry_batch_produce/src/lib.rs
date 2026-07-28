//! Public, fail-closed production entrypoint for one frozen XRY task subject.

use std::fmt;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};
use lightflow_xry_gateway::{GatewayError, GatewayRequest, invoke};

pub const WORKFLOW_ID: &str = "lightflow.xry_batch_produce";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

const INPUTS: &[&str] = &["task", "subject"];
const IMPLEMENTATION: &str = concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION"));

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "XRY Batch Produce",
        description: "Run the canonical XRY production chain for one frozen task subject through the locked gateway.",
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
        output "worker_context": "json" {
            description: "Canonical, bound worker context returned only after verified gateway PASS.",
        }
        output "production_report": "json" {
            description: "Opaque canonical production report returned only after verified gateway PASS.",
        }
        output "task_state_path": "text" {
            description: "Canonical task-state path reported by the XRY gateway.",
        }
        output "summary": "text" {
            description: "Verified canonical production outcome summary.",
        }
    }
    .builtin_runtime("command", "lightflow.command.run", "process.command.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, ProduceError> {
    let request = request_from_inputs(inputs)?;
    let response = invoke(&request).map_err(ProduceError::from)?;
    let result = response.production_result().map_err(ProduceError::from)?;
    let replay_fingerprint = with_package_implementation(
        response
            .replay_fingerprint()
            .as_object()
            .cloned()
            .ok_or_else(|| ProduceError::new("gateway replay fingerprint must be an object"))?,
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("worker_context".to_owned(), result.worker_context),
            ("production_report".to_owned(), result.production_report),
            ("task_state_path".to_owned(), result.task_state_path.into()),
            (
                "summary".to_owned(),
                "Canonical production PASS was verified for the bound XRY task subject.".into(),
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

fn request_from_inputs(inputs: &Map<String, Value>) -> Result<GatewayRequest, ProduceError> {
    if !inputs.keys().all(|name| INPUTS.contains(&name.as_str())) {
        return Err(ProduceError::new(
            "production request contains an unsupported input",
        ));
    }
    let task = required_text(inputs, "task")?;
    let subject = required_text(inputs, "subject")?;
    GatewayRequest::produce(task, subject).map_err(ProduceError::from)
}

fn required_text<'a>(inputs: &'a Map<String, Value>, name: &str) -> Result<&'a str, ProduceError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ProduceError::new("production request has a missing or invalid required text input")
        })
}

#[derive(Debug)]
pub struct ProduceError(String);

impl ProduceError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<GatewayError> for ProduceError {
    fn from(error: GatewayError) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for ProduceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ProduceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::json;

    fn valid_inputs() -> Map<String, Value> {
        Map::from_iter([
            (
                "task".to_owned(),
                json!("批量剪辑/皮卡严选 走全球/7.23批量"),
            ),
            ("subject".to_owned(), json!("S01")),
        ])
    }

    #[test]
    fn definition_uses_the_command_runtime_and_only_bound_inputs() {
        let definition = define();
        assert_eq!(definition.id, WORKFLOW_ID);
        assert_eq!(definition.version, WORKFLOW_VERSION);
        assert_eq!(definition.inputs.len(), INPUTS.len());
        assert_eq!(definition.runtimes.len(), 1);
        assert_eq!(definition.runtimes[0].id, "command");
        assert_eq!(definition.runtimes[0].capability, "lightflow.command.run");
        assert_eq!(
            definition.runtimes[0].engine.as_deref(),
            Some("process.command.v1")
        );
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
    fn request_rejects_legacy_stage_or_package_controls() {
        assert!(request_from_inputs(&valid_inputs()).is_ok());

        let mut legacy = valid_inputs();
        legacy.insert("commit_package".to_owned(), json!(true));
        assert!(request_from_inputs(&legacy).is_err());

        let mut missing = valid_inputs();
        missing.remove("subject");
        assert!(request_from_inputs(&missing).is_err());
    }
}
