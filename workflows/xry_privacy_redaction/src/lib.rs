//! Public, fail-closed pre-delivery entrypoint for canonical XRY person redaction.

use std::fmt;

use lightflow::preload::*;
use lightflow::runner::Response;
use lightflow::serde_json::{Map, Value};
use lightflow_xry_gateway::{
    GatewayError, GatewayRequest, REDACTION_POLICY_VERSION, RedactionState, invoke,
};

pub const WORKFLOW_ID: &str = "lightflow.xry_privacy_redaction";
pub const WORKFLOW_VERSION: &str = env!("CARGO_PKG_VERSION");

const INPUTS: &[&str] = &[
    "task",
    "subject",
    "apply",
    "plan_sha256",
    "confirmation_receipt_ref",
];
const IMPLEMENTATION: &str = concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION"));

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "XRY Privacy Redaction",
        description: "Preview or apply the canonical person-redaction stage for one frozen XRY task subject before delivery.",
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
            description: "False creates a review-required redaction preview. True requires one exact approved plan and opaque user-confirmation receipt.",
            required: false,
            default: false,
            widget: "toggle",
        }
        input "plan_sha256": "text" {
            description: "Exact approval plan SHA-256 required only when apply is true.",
            required: false,
            widget: "text",
        }
        input "confirmation_receipt_ref": "text" {
            description: "Opaque backend-verifiable user confirmation required only when apply is true.",
            required: false,
            widget: "text",
        }
        output "redaction_state": "text" {
            description: "Canonical redaction state: REVIEW_REQUIRED or APPLIED.",
        }
        output "policy_version": "text" {
            description: "Fixed canonical person-redaction policy version.",
        }
        output "approval_plan_sha256": "text" {
            description: "Exact opaque-backend approval plan SHA-256 for review or apply binding.",
        }
        output "review_packet_ref": "text" {
            description: "Opaque reference to the canonical low-resolution review packet.",
        }
        output "preview_receipt_ref": "text" {
            description: "Opaque reference to the canonical preview receipt.",
        }
        output "model_receipt_ref": "text" {
            description: "Opaque reference to the canonical model/runtime receipt.",
        }
        output "summary": "text" {
            description: "Conservative verified privacy-stage outcome summary.",
        }
    }
    .builtin_runtime("runner", "lightflow.runner", "runner.v1")
    .build()
}

pub fn execute(inputs: &Map<String, Value>) -> Result<Response, PrivacyRedactionError> {
    let request = request_from_inputs(inputs)?;
    let response = invoke(&request).map_err(PrivacyRedactionError::from)?;
    let result = response
        .redaction_result()
        .map_err(PrivacyRedactionError::from)?;
    let (redaction_state, summary) = match result.state() {
        RedactionState::ReviewRequired => (
            "REVIEW_REQUIRED",
            "Privacy-redaction preview is REVIEW_REQUIRED; no downstream outcome is asserted.",
        ),
        RedactionState::Applied => (
            "APPLIED",
            "Canonical privacy-redaction PASS was verified for the bound XRY task subject.",
        ),
    };
    let replay_fingerprint = with_package_implementation(
        response
            .replay_fingerprint()
            .as_object()
            .cloned()
            .ok_or_else(|| {
                PrivacyRedactionError::new("gateway replay fingerprint must be an object")
            })?,
    );
    Ok(Response {
        outputs: Map::from_iter([
            ("redaction_state".to_owned(), redaction_state.into()),
            (
                "policy_version".to_owned(),
                REDACTION_POLICY_VERSION.to_owned().into(),
            ),
            (
                "approval_plan_sha256".to_owned(),
                result.approval_plan_sha256().to_owned().into(),
            ),
            (
                "review_packet_ref".to_owned(),
                result.review_packet_ref().as_str().to_owned().into(),
            ),
            (
                "preview_receipt_ref".to_owned(),
                result.preview_receipt_ref().as_str().to_owned().into(),
            ),
            (
                "model_receipt_ref".to_owned(),
                result.model_receipt_ref().as_str().to_owned().into(),
            ),
            ("summary".to_owned(), summary.into()),
        ]),
        artifacts: Vec::new(),
        replay_fingerprint,
    })
}

fn with_package_implementation(mut fingerprint: Map<String, Value>) -> Map<String, Value> {
    fingerprint.insert("implementation".to_owned(), IMPLEMENTATION.into());
    fingerprint
}

fn request_from_inputs(
    inputs: &Map<String, Value>,
) -> Result<GatewayRequest, PrivacyRedactionError> {
    reject_unknown_inputs(inputs)?;
    let task = required_text(inputs, "task")?;
    let subject = required_text(inputs, "subject")?;
    let apply = optional_boolean(inputs, "apply", false)?;
    let plan_sha256 = optional_text(inputs, "plan_sha256")?;
    let confirmation_receipt_ref = optional_text(inputs, "confirmation_receipt_ref")?;
    GatewayRequest::redact(task, subject, apply, plan_sha256, confirmation_receipt_ref)
        .map_err(PrivacyRedactionError::from)
}

fn reject_unknown_inputs(inputs: &Map<String, Value>) -> Result<(), PrivacyRedactionError> {
    if inputs.keys().all(|name| INPUTS.contains(&name.as_str())) {
        Ok(())
    } else {
        Err(PrivacyRedactionError::new(
            "privacy-redaction request contains an unsupported input",
        ))
    }
}

fn required_text<'a>(
    inputs: &'a Map<String, Value>,
    name: &str,
) -> Result<&'a str, PrivacyRedactionError> {
    inputs
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PrivacyRedactionError::new(
                "privacy-redaction request has a missing or invalid required text input",
            )
        })
}

fn optional_text(
    inputs: &Map<String, Value>,
    name: &str,
) -> Result<Option<String>, PrivacyRedactionError> {
    match inputs.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        _ => Err(PrivacyRedactionError::new(
            "privacy-redaction request has an invalid optional text input",
        )),
    }
}

fn optional_boolean(
    inputs: &Map<String, Value>,
    name: &str,
    default: bool,
) -> Result<bool, PrivacyRedactionError> {
    match inputs.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(PrivacyRedactionError::new(
            "privacy-redaction request has an invalid optional boolean input",
        )),
    }
}

#[derive(Debug)]
pub struct PrivacyRedactionError(String);

impl PrivacyRedactionError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<GatewayError> for PrivacyRedactionError {
    fn from(error: GatewayError) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for PrivacyRedactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PrivacyRedactionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::serde_json::{json, to_value};

    const PLAN_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CONFIRMATION_RECEIPT_REF: &str =
        "opaque:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn valid_preview_inputs() -> Map<String, Value> {
        Map::from_iter([
            (
                "task".to_owned(),
                json!("批量剪辑/皮卡严选 走全球/7.23批量"),
            ),
            ("subject".to_owned(), json!("S01")),
        ])
    }

    #[test]
    fn definition_uses_the_package_identity_and_only_bound_inputs() {
        let definition = define();
        assert_eq!(definition.id, WORKFLOW_ID);
        assert_eq!(definition.version, WORKFLOW_VERSION);
        assert_eq!(
            INPUTS,
            [
                "task",
                "subject",
                "apply",
                "plan_sha256",
                "confirmation_receipt_ref",
            ]
            .as_slice()
        );
        assert_eq!(definition.inputs.len(), INPUTS.len());
        assert_eq!(definition.runtimes.len(), 1);
        assert_eq!(definition.runtimes[0].id, "runner");
        assert_eq!(definition.runtimes[0].capability, "lightflow.runner");
        assert_eq!(definition.runtimes[0].engine.as_deref(), Some("runner.v1"));
    }

    #[test]
    fn preview_request_accepts_only_the_bound_preview_tuple() {
        let request = request_from_inputs(&valid_preview_inputs()).expect("bound preview request");
        let encoded = to_value(&request).expect("request encoding");
        assert_eq!(encoded["action"], json!("redact"));
        assert_eq!(encoded["apply"], json!(false));
        assert_eq!(encoded["plan_sha256"], Value::Null);
        assert_eq!(encoded["confirmation_receipt_ref"], Value::Null);
    }

    #[test]
    fn request_rejects_unknown_inputs_and_invalid_apply_tuples() {
        let mut unknown = valid_preview_inputs();
        unknown.insert("model_args".to_owned(), json!(["person"]));
        assert!(request_from_inputs(&unknown).is_err());

        let mut non_boolean_apply = valid_preview_inputs();
        non_boolean_apply.insert("apply".to_owned(), json!("true"));
        assert!(request_from_inputs(&non_boolean_apply).is_err());

        let mut preview_with_plan = valid_preview_inputs();
        preview_with_plan.insert("plan_sha256".to_owned(), json!(PLAN_SHA256));
        assert!(request_from_inputs(&preview_with_plan).is_err());

        let mut apply_without_confirmation = valid_preview_inputs();
        apply_without_confirmation.insert("apply".to_owned(), json!(true));
        apply_without_confirmation.insert("plan_sha256".to_owned(), json!(PLAN_SHA256));
        assert!(request_from_inputs(&apply_without_confirmation).is_err());

        let mut confirmed_apply = apply_without_confirmation;
        confirmed_apply.insert(
            "confirmation_receipt_ref".to_owned(),
            json!(CONFIRMATION_RECEIPT_REF),
        );
        assert!(request_from_inputs(&confirmed_apply).is_ok());
    }

    #[test]
    fn replay_fingerprint_identifies_this_package_runner() {
        let fingerprint = with_package_implementation(Map::new());
        assert_eq!(
            fingerprint.get("implementation"),
            Some(&Value::String(IMPLEMENTATION.to_owned()))
        );
    }
}
