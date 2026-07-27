use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol_frame;

pub const PROTOCOL_VERSION: &str = "lightflow.xry.gateway.v1";
pub const SUBSYSTEM_NAME: &str = "lightflow-xry-gateway-v1";
pub(crate) use crate::protocol_frame::MAX_FRAME_BYTES;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayError(String);

impl GatewayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GatewayError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    Progress,
    Freeze,
    Cleanup,
    Archive,
}

impl ControlAction {
    pub fn parse(value: &str) -> Result<Self, GatewayError> {
        match value {
            "progress" => Ok(Self::Progress),
            "freeze" => Ok(Self::Freeze),
            "cleanup" => Ok(Self::Cleanup),
            "archive" => Ok(Self::Archive),
            _ => Err(GatewayError::new(
                "action must be one of progress, freeze, cleanup, or archive",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Freeze => "freeze",
            Self::Cleanup => "cleanup",
            Self::Archive => "archive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAction {
    Progress,
    Freeze,
    Cleanup,
    Archive,
    Produce,
}

impl GatewayAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Freeze => "freeze",
            Self::Cleanup => "cleanup",
            Self::Archive => "archive",
            Self::Produce => "produce",
        }
    }
}

impl From<ControlAction> for GatewayAction {
    fn from(action: ControlAction) -> Self {
        match action {
            ControlAction::Progress => Self::Progress,
            ControlAction::Freeze => Self::Freeze,
            ControlAction::Cleanup => Self::Cleanup,
            ControlAction::Archive => Self::Archive,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayRequest {
    protocol: &'static str,
    request_id: String,
    request_sha256: String,
    action: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: Option<String>,
}

#[derive(Serialize)]
struct UnsignedGatewayRequest<'a> {
    protocol: &'static str,
    request_id: &'a str,
    action: GatewayAction,
    task: &'a str,
    subject: &'a str,
    apply: bool,
    plan_sha256: Option<&'a str>,
}

impl GatewayRequest {
    pub fn control(
        action: ControlAction,
        task: impl Into<String>,
        subject: impl Into<String>,
        apply: bool,
        plan_sha256: Option<String>,
    ) -> Result<Self, GatewayError> {
        let action = GatewayAction::from(action);
        match action {
            GatewayAction::Progress | GatewayAction::Freeze => {
                if apply || plan_sha256.is_some() {
                    return Err(GatewayError::new(
                        "progress and freeze do not accept apply or plan_sha256",
                    ));
                }
            }
            GatewayAction::Cleanup | GatewayAction::Archive => {
                if apply && plan_sha256.is_none() {
                    return Err(GatewayError::new(
                        "cleanup and archive require plan_sha256 when apply is true",
                    ));
                }
            }
            GatewayAction::Produce => unreachable!("control actions never map to produce"),
        }
        Self::new(action, task.into(), subject.into(), apply, plan_sha256)
    }

    pub fn produce(
        task: impl Into<String>,
        subject: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        Self::new(
            GatewayAction::Produce,
            task.into(),
            subject.into(),
            false,
            None,
        )
    }

    fn new(
        action: GatewayAction,
        task: String,
        subject: String,
        apply: bool,
        plan_sha256: Option<String>,
    ) -> Result<Self, GatewayError> {
        validate_task(&task)?;
        validate_subject(&subject)?;
        if let Some(plan_sha256) = plan_sha256.as_deref() {
            validate_sha256(plan_sha256, "plan_sha256")?;
        }
        if action == GatewayAction::Produce && (apply || plan_sha256.is_some()) {
            return Err(GatewayError::new(
                "produce does not accept apply or plan_sha256",
            ));
        }

        let request_id = next_request_id();
        let unsigned = UnsignedGatewayRequest {
            protocol: PROTOCOL_VERSION,
            request_id: &request_id,
            action,
            task: &task,
            subject: &subject,
            apply,
            plan_sha256: plan_sha256.as_deref(),
        };
        let request_sha256 = protocol_frame::request_sha256(&unsigned)?;
        Ok(Self {
            protocol: PROTOCOL_VERSION,
            request_id,
            request_sha256,
            action,
            task,
            subject,
            apply,
            plan_sha256,
        })
    }

    pub fn action(&self) -> GatewayAction {
        self.action
    }

    pub fn task(&self) -> &str {
        &self.task
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    fn apply(&self) -> bool {
        self.apply
    }

    fn plan_sha256(&self) -> Option<&str> {
        self.plan_sha256.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayResponse {
    request_sha256: String,
    action: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: Option<String>,
    gateway_identity: String,
    receipt_sha256: String,
    report: Value,
}

#[derive(Debug, Clone)]
pub struct ProductionResult {
    pub worker_context: Value,
    pub production_report: Value,
    pub task_state_path: String,
}

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
    plan_sha256: Value,
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
        })
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

pub(crate) fn encode_request_frame(request: &GatewayRequest) -> Result<Vec<u8>, GatewayError> {
    protocol_frame::encode_frame(request)
}

pub(crate) fn decode_response_frame(
    bytes: &[u8],
    request: &GatewayRequest,
) -> Result<GatewayResponse, GatewayError> {
    let envelope: GatewayResponseEnvelope = protocol_frame::decode_frame(bytes)?;
    if envelope.protocol != PROTOCOL_VERSION {
        return Err(GatewayError::new("gateway response protocol mismatch"));
    }
    if envelope.request_id != request.request_id {
        return Err(GatewayError::new("gateway response request_id mismatch"));
    }
    if envelope.request_sha256 != request.request_sha256 {
        return Err(GatewayError::new(
            "gateway response request_sha256 mismatch",
        ));
    }
    if envelope.action != request.action || envelope.stage != request.action {
        return Err(GatewayError::new(
            "gateway response action or stage mismatch",
        ));
    }
    if envelope.task != request.task || envelope.subject != request.subject {
        return Err(GatewayError::new(
            "gateway response task or subject mismatch",
        ));
    }
    if envelope.apply != request.apply() {
        return Err(GatewayError::new("gateway response apply mismatch"));
    }
    let plan_sha256 = nullable_sha256(&envelope.plan_sha256)?;
    if plan_sha256.as_deref() != request.plan_sha256() {
        return Err(GatewayError::new("gateway response plan_sha256 mismatch"));
    }
    let GatewayStatus::Pass = envelope.status;
    if !valid_nonempty_text(&envelope.gateway_identity, 256) {
        return Err(GatewayError::new("gateway identity is invalid"));
    }
    validate_sha256(&envelope.receipt_sha256, "receipt_sha256")?;
    if !envelope.report.is_object() {
        return Err(GatewayError::new("canonical report must be an object"));
    }
    Ok(GatewayResponse {
        request_sha256: envelope.request_sha256,
        action: envelope.action,
        task: envelope.task,
        subject: envelope.subject,
        apply: envelope.apply,
        plan_sha256,
        gateway_identity: envelope.gateway_identity,
        receipt_sha256: envelope.receipt_sha256,
        report: envelope.report,
    })
}

fn validate_task(task: &str) -> Result<(), GatewayError> {
    if task.trim() != task {
        return Err(GatewayError::new(
            "task must not have surrounding whitespace",
        ));
    }
    let mut segments = task.split('/');
    let (Some(prefix), Some(group), Some(batch), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return Err(GatewayError::new(
            "task must be exactly 批量剪辑/<group>/<batch>",
        ));
    };
    if prefix != "批量剪辑" || !valid_path_segment(group) || !valid_path_segment(batch) {
        return Err(GatewayError::new(
            "task must be exactly a safe 批量剪辑/<group>/<batch> binding",
        ));
    }
    Ok(())
}

fn validate_subject(subject: &str) -> Result<(), GatewayError> {
    let bytes = subject.as_bytes();
    if bytes.len() != 3
        || bytes[0] != b'S'
        || !bytes[1..].iter().all(u8::is_ascii_digit)
        || subject == "S00"
    {
        return Err(GatewayError::new(
            "subject must be an exact S01 through S99 binding",
        ));
    }
    Ok(())
}

fn valid_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 255
        && segment.trim() == segment
        && segment != "."
        && segment != ".."
        && !segment.contains(['\\', '\0'])
        && !segment.chars().any(char::is_control)
}

fn valid_nonempty_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= maximum_bytes
        && !value.chars().any(char::is_control)
}

fn validate_sha256(value: &str, name: &str) -> Result<(), GatewayError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(GatewayError::new(format!(
            "{name} must be 64 lowercase hexadecimal characters"
        )))
    }
}

fn nullable_sha256(value: &Value) -> Result<Option<String>, GatewayError> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => {
            validate_sha256(value, "plan_sha256")?;
            Ok(Some(value.clone()))
        }
        _ => Err(GatewayError::new(
            "gateway response plan_sha256 must be a string or null",
        )),
    }
}

fn next_request_id() -> String {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("lfw-xry-{milliseconds:016x}-{sequence:016x}")
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
