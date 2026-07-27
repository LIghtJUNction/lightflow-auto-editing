use serde::Deserialize;
use serde_json::{Value, json};

use crate::protocol::{
    GatewayAction, GatewayError, GatewayRequest, OpaqueReference, PROTOCOL_VERSION, validate_sha256,
};
use crate::protocol_frame;

#[derive(Debug, Clone)]
pub struct GatewayResponse {
    request_sha256: String,
    action: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: Option<String>,
    confirmation_receipt_ref: Option<OpaqueReference>,
    approval_plan_sha256: Option<String>,
    gateway_identity: String,
    receipt_sha256: String,
    report: Value,
    redaction_result: Option<RedactionResult>,
}

#[derive(Debug, Clone)]
pub struct ProductionResult {
    pub worker_context: Value,
    pub production_report: Value,
    pub task_state_path: String,
}

pub const REDACTION_POLICY_VERSION: &str = "xry-person-redaction.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedactionState {
    ReviewRequired,
    Applied,
}

/// A fully validated redaction outcome. It intentionally exposes opaque receipts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionResult {
    state: RedactionState,
    approval_plan_sha256: String,
    review_packet_ref: OpaqueReference,
    preview_receipt_ref: OpaqueReference,
    model_receipt_ref: OpaqueReference,
}

impl RedactionResult {
    pub const fn state(&self) -> RedactionState {
        self.state
    }

    pub fn approval_plan_sha256(&self) -> &str {
        &self.approval_plan_sha256
    }

    pub fn review_packet_ref(&self) -> &OpaqueReference {
        &self.review_packet_ref
    }

    pub fn preview_receipt_ref(&self) -> &OpaqueReference {
        &self.preview_receipt_ref
    }

    pub fn model_receipt_ref(&self) -> &OpaqueReference {
        &self.model_receipt_ref
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RedactionReportEnvelope {
    state: RedactionState,
    policy_version: String,
    approval_plan_sha256: String,
    review_packet_ref: String,
    preview_receipt_ref: String,
    model_receipt_ref: String,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct RequiredNullableSha256(Option<String>);

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct RequiredNullableOpaqueReference(Option<String>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GatewayResponseEnvelope {
    protocol: String,
    request_id: String,
    request_sha256: String,
    action: GatewayAction,
    stage: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: RequiredNullableSha256,
    confirmation_receipt_ref: RequiredNullableOpaqueReference,
    approval_plan_sha256: RequiredNullableSha256,
    status: GatewayStatus,
    gateway_identity: String,
    receipt_sha256: String,
    report: Value,
}

#[derive(Debug, Deserialize)]
enum GatewayStatus {
    #[serde(rename = "PASS")]
    Pass,
}

impl GatewayResponse {
    pub fn report(&self) -> &Value {
        &self.report
    }

    pub fn replay_fingerprint(&self) -> Value {
        json!({
            "gateway_protocol": PROTOCOL_VERSION,
            "gateway_identity": self.gateway_identity,
            "receipt_sha256": self.receipt_sha256,
            "request_sha256": self.request_sha256,
            "action": self.action.as_str(),
            "task": self.task,
            "subject": self.subject,
            "apply": self.apply,
            "plan_sha256": self.plan_sha256,
            "confirmation_receipt_ref": self
                .confirmation_receipt_ref
                .as_ref()
                .map(OpaqueReference::as_str),
            "approval_plan_sha256": self.approval_plan_sha256,
        })
    }

    pub fn approval_plan_sha256(&self) -> Option<&str> {
        self.approval_plan_sha256.as_deref()
    }

    pub fn redaction_result(&self) -> Result<&RedactionResult, GatewayError> {
        if self.action != GatewayAction::Redact {
            return Err(GatewayError::new(
                "only a canonical redact response can contain a redaction result",
            ));
        }
        self.redaction_result
            .as_ref()
            .ok_or_else(|| GatewayError::new("redact response lacks typed redaction result"))
    }

    pub fn production_result(&self) -> Result<ProductionResult, GatewayError> {
        if self.action != GatewayAction::Produce {
            return Err(GatewayError::new(
                "only a canonical produce response can contain worker context",
            ));
        }
        let report = self
            .report
            .as_object()
            .ok_or_else(|| GatewayError::new("canonical report must be an object"))?;
        let worker_context = report
            .get("worker_context")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or_else(|| GatewayError::new("produce report lacks worker_context object"))?;
        let production_report = report
            .get("production_report")
            .filter(|value| value.is_object())
            .cloned()
            .ok_or_else(|| GatewayError::new("produce report lacks production_report object"))?;
        let task_state_path = report
            .get("task_state_path")
            .and_then(Value::as_str)
            .filter(|value| valid_nonempty_text(value, 1024))
            .map(ToOwned::to_owned)
            .ok_or_else(|| GatewayError::new("produce report lacks task_state_path"))?;
        Ok(ProductionResult {
            worker_context,
            production_report,
            task_state_path,
        })
    }
}

pub(crate) fn decode_response_frame(
    bytes: &[u8],
    request: &GatewayRequest,
) -> Result<GatewayResponse, GatewayError> {
    let envelope: GatewayResponseEnvelope = protocol_frame::decode_frame(bytes)?;
    if envelope.protocol != PROTOCOL_VERSION {
        return Err(GatewayError::new("gateway response protocol mismatch"));
    }
    if envelope.request_id != request.request_id() {
        return Err(GatewayError::new("gateway response request_id mismatch"));
    }
    if envelope.request_sha256 != request.request_sha256() {
        return Err(GatewayError::new(
            "gateway response request_sha256 mismatch",
        ));
    }
    if envelope.action != request.action() || envelope.stage != request.action() {
        return Err(GatewayError::new(
            "gateway response action or stage mismatch",
        ));
    }
    if envelope.task != request.task() || envelope.subject != request.subject() {
        return Err(GatewayError::new(
            "gateway response task or subject mismatch",
        ));
    }
    if envelope.apply != request.apply() {
        return Err(GatewayError::new("gateway response apply mismatch"));
    }
    let plan_sha256 = nullable_sha256(envelope.plan_sha256.0, "plan_sha256")?;
    if plan_sha256.as_deref() != request.plan_sha256() {
        return Err(GatewayError::new("gateway response plan_sha256 mismatch"));
    }
    let confirmation_receipt_ref = nullable_opaque_reference(envelope.confirmation_receipt_ref.0)?;
    if confirmation_receipt_ref
        .as_ref()
        .map(OpaqueReference::as_str)
        != request.confirmation_receipt_ref()
    {
        return Err(GatewayError::new(
            "gateway response confirmation_receipt_ref mismatch",
        ));
    }
    let approval_plan_sha256 =
        nullable_sha256(envelope.approval_plan_sha256.0, "approval_plan_sha256")?;
    let GatewayStatus::Pass = envelope.status;
    if !valid_nonempty_text(&envelope.gateway_identity, 256) {
        return Err(GatewayError::new("gateway identity is invalid"));
    }
    validate_sha256(&envelope.receipt_sha256, "receipt_sha256")?;
    if !envelope.report.is_object() {
        return Err(GatewayError::new("canonical report must be an object"));
    }
    let redaction_result = match envelope.action {
        GatewayAction::Redact => {
            let result = parse_redaction_result(&envelope.report)?;
            if approval_plan_sha256.as_deref() != Some(result.approval_plan_sha256()) {
                return Err(GatewayError::new(
                    "redact response approval_plan_sha256 mismatch",
                ));
            }
            match (
                request.apply(),
                request.plan_sha256(),
                request.confirmation_receipt_ref(),
                result.state(),
            ) {
                (false, None, None, RedactionState::ReviewRequired) => {}
                (true, Some(plan_sha256), Some(_), RedactionState::Applied)
                    if result.approval_plan_sha256() == plan_sha256 => {}
                _ => {
                    return Err(GatewayError::new(
                        "redact response does not match the request contract",
                    ));
                }
            }
            Some(result)
        }
        _ => {
            if approval_plan_sha256.is_some() {
                return Err(GatewayError::new(
                    "only a redact response can contain approval_plan_sha256",
                ));
            }
            None
        }
    };
    Ok(GatewayResponse {
        request_sha256: envelope.request_sha256,
        action: envelope.action,
        task: envelope.task,
        subject: envelope.subject,
        apply: envelope.apply,
        plan_sha256,
        confirmation_receipt_ref,
        approval_plan_sha256,
        gateway_identity: envelope.gateway_identity,
        receipt_sha256: envelope.receipt_sha256,
        report: envelope.report,
        redaction_result,
    })
}

fn valid_nonempty_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= maximum_bytes
        && !value.chars().any(char::is_control)
}

fn nullable_sha256(value: Option<String>, name: &str) -> Result<Option<String>, GatewayError> {
    if let Some(value) = value.as_deref() {
        validate_sha256(value, name)?;
    }
    Ok(value)
}

fn nullable_opaque_reference(
    value: Option<String>,
) -> Result<Option<OpaqueReference>, GatewayError> {
    value.map(OpaqueReference::new).transpose()
}

fn parse_redaction_result(report: &Value) -> Result<RedactionResult, GatewayError> {
    let wire: RedactionReportEnvelope = serde_json::from_value(report.clone()).map_err(|_| {
        GatewayError::new("redact response report must be an exact typed redaction result")
    })?;
    if wire.policy_version != REDACTION_POLICY_VERSION {
        return Err(GatewayError::new(
            "redact response policy version is invalid",
        ));
    }
    validate_sha256(&wire.approval_plan_sha256, "approval_plan_sha256")?;
    Ok(RedactionResult {
        state: wire.state,
        approval_plan_sha256: wire.approval_plan_sha256,
        review_packet_ref: OpaqueReference::new(wire.review_packet_ref)?,
        preview_receipt_ref: OpaqueReference::new(wire.preview_receipt_ref)?,
        model_receipt_ref: OpaqueReference::new(wire.model_receipt_ref)?,
    })
}
