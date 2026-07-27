use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::protocol_frame;

pub const PROTOCOL_VERSION: &str = "lightflow.xry.gateway.v1";
pub const SUBSYSTEM_NAME: &str = "lightflow-xry-gateway-v1";
pub(crate) use crate::protocol_frame::MAX_FRAME_BYTES;
pub use crate::protocol_response::{
    GatewayResponse, ProductionResult, REDACTION_POLICY_VERSION, RedactionResult, RedactionState,
};

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
    Redact,
}

impl GatewayAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Freeze => "freeze",
            Self::Cleanup => "cleanup",
            Self::Archive => "archive",
            Self::Produce => "produce",
            Self::Redact => "redact",
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

/// A bounded receipt reference that deliberately cannot carry a path, command, or free-form text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct OpaqueReference(String);

impl OpaqueReference {
    pub fn new(value: impl Into<String>) -> Result<Self, GatewayError> {
        let value = value.into();
        validate_opaque_reference(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
    confirmation_receipt_ref: Option<OpaqueReference>,
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
    confirmation_receipt_ref: Option<&'a str>,
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
            GatewayAction::Produce | GatewayAction::Redact => {
                unreachable!("control actions never map to produce or redact")
            }
        }
        Self::new(
            action,
            task.into(),
            subject.into(),
            apply,
            plan_sha256,
            None,
        )
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
            None,
        )
    }

    pub fn redact(
        task: impl Into<String>,
        subject: impl Into<String>,
        apply: bool,
        plan_sha256: Option<String>,
        confirmation_receipt_ref: Option<String>,
    ) -> Result<Self, GatewayError> {
        Self::new(
            GatewayAction::Redact,
            task.into(),
            subject.into(),
            apply,
            plan_sha256,
            confirmation_receipt_ref,
        )
    }

    fn new(
        action: GatewayAction,
        task: String,
        subject: String,
        apply: bool,
        plan_sha256: Option<String>,
        confirmation_receipt_ref: Option<String>,
    ) -> Result<Self, GatewayError> {
        validate_task(&task)?;
        validate_subject(&subject)?;
        if let Some(plan_sha256) = plan_sha256.as_deref() {
            validate_sha256(plan_sha256, "plan_sha256")?;
        }
        let confirmation_receipt_ref = confirmation_receipt_ref
            .map(OpaqueReference::new)
            .transpose()?;
        validate_action_contract(
            action,
            apply,
            plan_sha256.as_deref(),
            confirmation_receipt_ref
                .as_ref()
                .map(OpaqueReference::as_str),
        )?;

        let request_id = next_request_id();
        let unsigned = UnsignedGatewayRequest {
            protocol: PROTOCOL_VERSION,
            request_id: &request_id,
            action,
            task: &task,
            subject: &subject,
            apply,
            plan_sha256: plan_sha256.as_deref(),
            confirmation_receipt_ref: confirmation_receipt_ref
                .as_ref()
                .map(OpaqueReference::as_str),
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
            confirmation_receipt_ref,
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

    pub(crate) fn apply(&self) -> bool {
        self.apply
    }

    pub(crate) fn plan_sha256(&self) -> Option<&str> {
        self.plan_sha256.as_deref()
    }

    pub(crate) fn confirmation_receipt_ref(&self) -> Option<&str> {
        self.confirmation_receipt_ref
            .as_ref()
            .map(OpaqueReference::as_str)
    }
}

pub(crate) fn encode_request_frame(request: &GatewayRequest) -> Result<Vec<u8>, GatewayError> {
    protocol_frame::encode_frame(request)
}

pub(crate) fn decode_response_frame(
    bytes: &[u8],
    request: &GatewayRequest,
) -> Result<GatewayResponse, GatewayError> {
    crate::protocol_response::decode_response_frame(bytes, request)
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

pub(crate) fn validate_sha256(value: &str, name: &str) -> Result<(), GatewayError> {
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

fn validate_action_contract(
    action: GatewayAction,
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
) -> Result<(), GatewayError> {
    match action {
        GatewayAction::Progress | GatewayAction::Freeze
            if apply || plan_sha256.is_some() || confirmation_receipt_ref.is_some() =>
        {
            Err(GatewayError::new(
                "progress and freeze do not accept apply, plan_sha256, or confirmation_receipt_ref",
            ))
        }
        GatewayAction::Cleanup | GatewayAction::Archive
            if confirmation_receipt_ref.is_some() || (apply && plan_sha256.is_none()) =>
        {
            Err(GatewayError::new(
                "cleanup and archive require plan_sha256 when apply is true and reject confirmation_receipt_ref",
            ))
        }
        GatewayAction::Produce
            if apply || plan_sha256.is_some() || confirmation_receipt_ref.is_some() =>
        {
            Err(GatewayError::new(
                "produce does not accept apply, plan_sha256, or confirmation_receipt_ref",
            ))
        }
        GatewayAction::Redact
            if !matches!(
                (
                    apply,
                    plan_sha256.is_some(),
                    confirmation_receipt_ref.is_some()
                ),
                (false, false, false) | (true, true, true)
            ) =>
        {
            Err(GatewayError::new(
                "redact preview requires null plan_sha256 and confirmation_receipt_ref; apply requires both",
            ))
        }
        _ => Ok(()),
    }
}

fn validate_opaque_reference(value: &str) -> Result<(), GatewayError> {
    let Some(hash) = value.strip_prefix("opaque:") else {
        return Err(GatewayError::new(
            "opaque references must use the opaque:<sha256> form",
        ));
    };
    validate_sha256(hash, "opaque reference")
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
