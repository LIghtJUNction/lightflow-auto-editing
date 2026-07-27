//! Shared, fail-closed framing and request validation for the XRY gateway v1 wire protocol.
//!
//! This crate intentionally knows only a bounded request tuple. It has no filesystem,
//! environment, subprocess, network, or canonical-backend integration.

use std::fmt;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_VERSION: &str = "lightflow.xry.gateway.v1";
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

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
    pub const fn as_str(self) -> &'static str {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolError(&'static str);

impl ProtocolError {
    const fn new(message: &'static str) -> Self {
        Self(message)
    }

    pub const fn message(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ProtocolError {}

/// The only data a handler may pass to a canonical backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRequest {
    request_id: String,
    request_sha256: String,
    action: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: Option<String>,
    confirmation_receipt_ref: Option<String>,
}

impl ValidatedRequest {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub const fn action(&self) -> GatewayAction {
        self.action
    }

    pub fn task(&self) -> &str {
        &self.task
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub const fn apply(&self) -> bool {
        self.apply
    }

    pub fn plan_sha256(&self) -> Option<&str> {
        self.plan_sha256.as_deref()
    }

    pub fn confirmation_receipt_ref(&self) -> Option<&str> {
        self.confirmation_receipt_ref.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InboundRequest {
    protocol: String,
    request_id: String,
    request_sha256: String,
    action: GatewayAction,
    task: String,
    subject: String,
    apply: bool,
    plan_sha256: RequiredNullableSha256,
    confirmation_receipt_ref: RequiredNullableOpaqueReference,
}

/// A non-optional field whose JSON value is either a string or `null`.
/// Wrapping the option keeps omission distinct from the canonical v1 null value.
#[derive(Deserialize)]
#[serde(transparent)]
struct RequiredNullableSha256(Option<String>);

/// A non-optional field whose JSON value is an opaque reference string or `null`.
#[derive(Deserialize)]
#[serde(transparent)]
struct RequiredNullableOpaqueReference(Option<String>);

#[derive(Serialize)]
struct UnsignedRequest<'a> {
    protocol: &'static str,
    request_id: &'a str,
    action: GatewayAction,
    task: &'a str,
    subject: &'a str,
    apply: bool,
    plan_sha256: Option<&'a str>,
    confirmation_receipt_ref: Option<&'a str>,
}

/// Read one bounded request from a stream and validate its exact v1 binding.
///
/// The stream must close after the sole frame; any trailing byte is rejected.
pub fn read_and_validate_request(
    reader: &mut impl Read,
) -> Result<ValidatedRequest, ProtocolError> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_FRAME_BYTES as u64) + 5)
        .read_to_end(&mut bytes)
        .map_err(|_| ProtocolError::new("gateway request could not be read"))?;
    if bytes.len() > MAX_FRAME_BYTES + 4 {
        return Err(ProtocolError::new(
            "gateway request exceeds the protocol limit",
        ));
    }
    decode_and_validate_request_frame(&bytes)
}

/// Decode and validate an already-buffered complete v1 request frame.
pub fn decode_and_validate_request_frame(bytes: &[u8]) -> Result<ValidatedRequest, ProtocolError> {
    let payload = decode_single_frame(bytes, "gateway request")?;
    let wire: InboundRequest = serde_json::from_slice(payload)
        .map_err(|_| ProtocolError::new("gateway request JSON is invalid"))?;
    if wire.protocol != PROTOCOL_VERSION {
        return Err(ProtocolError::new("gateway request protocol is invalid"));
    }
    validate_request_id(&wire.request_id)?;
    validate_sha256(&wire.request_sha256)?;
    validate_task(&wire.task)?;
    validate_subject(&wire.subject)?;
    let plan_sha256 = wire.plan_sha256.0;
    if let Some(plan_sha256) = plan_sha256.as_deref() {
        validate_sha256(plan_sha256)?;
    }
    let confirmation_receipt_ref = wire.confirmation_receipt_ref.0;
    if let Some(confirmation_receipt_ref) = confirmation_receipt_ref.as_deref() {
        validate_opaque_reference(confirmation_receipt_ref)?;
    }
    validate_action_contract(
        wire.action,
        wire.apply,
        plan_sha256.as_deref(),
        confirmation_receipt_ref.as_deref(),
    )?;
    let expected_request_sha256 = request_sha256_for(
        &wire.request_id,
        wire.action,
        &wire.task,
        &wire.subject,
        wire.apply,
        plan_sha256.as_deref(),
        confirmation_receipt_ref.as_deref(),
    )?;
    if wire.request_sha256 != expected_request_sha256 {
        return Err(ProtocolError::new("gateway request hash is invalid"));
    }
    Ok(ValidatedRequest {
        request_id: wire.request_id,
        request_sha256: wire.request_sha256,
        action: wire.action,
        task: wire.task,
        subject: wire.subject,
        apply: wire.apply,
        plan_sha256,
        confirmation_receipt_ref,
    })
}

/// Encode one bounded, length-prefixed JSON frame.
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    let payload = canonical_json(value)?;
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::new(
            "gateway frame exceeds the protocol limit",
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| ProtocolError::new("gateway frame length is invalid"))?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Write one bounded protocol frame without adding a second record.
pub fn write_frame(writer: &mut impl Write, frame: &[u8]) -> Result<(), ProtocolError> {
    writer
        .write_all(frame)
        .and_then(|()| writer.flush())
        .map_err(|_| ProtocolError::new("gateway response could not be written"))
}

/// Recompute the v1 request hash over the exact unsigned field order used by the client.
pub fn request_sha256_for(
    request_id: &str,
    action: GatewayAction,
    task: &str,
    subject: &str,
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
) -> Result<String, ProtocolError> {
    let unsigned = UnsignedRequest {
        protocol: PROTOCOL_VERSION,
        request_id,
        action,
        task,
        subject,
        apply,
        plan_sha256,
        confirmation_receipt_ref,
    };
    Ok(sha256_hex(&canonical_json(&unsigned)?))
}

/// Validate the lowercase hexadecimal form used for request, plan, and receipt hashes.
pub fn validate_sha256(value: &str) -> Result<(), ProtocolError> {
    if is_lower_hex(value, 64) {
        Ok(())
    } else {
        Err(ProtocolError::new("gateway hash is invalid"))
    }
}

/// Validate the only public reference shape accepted by the gateway.
pub fn validate_opaque_reference(value: &str) -> Result<(), ProtocolError> {
    let Some(hash) = value.strip_prefix("opaque:") else {
        return Err(ProtocolError::new("gateway opaque reference is invalid"));
    };
    validate_sha256(hash).map_err(|_| ProtocolError::new("gateway opaque reference is invalid"))
}

fn decode_single_frame<'a>(bytes: &'a [u8], noun: &'static str) -> Result<&'a [u8], ProtocolError> {
    if bytes.len() < 4 {
        return Err(ProtocolError::new(match noun {
            "gateway request" => "gateway request has no complete frame header",
            _ => "gateway frame has no complete header",
        }));
    }
    let mut header = [0_u8; 4];
    header.copy_from_slice(&bytes[..4]);
    let declared = u32::from_be_bytes(header) as usize;
    if declared == 0 || declared > MAX_FRAME_BYTES {
        return Err(ProtocolError::new(match noun {
            "gateway request" => "gateway request frame length is invalid",
            _ => "gateway frame length is invalid",
        }));
    }
    let expected = 4_usize
        .checked_add(declared)
        .ok_or_else(|| ProtocolError::new("gateway frame length is invalid"))?;
    if bytes.len() != expected {
        return Err(ProtocolError::new(match noun {
            "gateway request" => "gateway request must contain exactly one complete frame",
            _ => "gateway frame must contain exactly one complete frame",
        }));
    }
    Ok(&bytes[4..])
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(value).map_err(|_| ProtocolError::new("gateway JSON serialization failed"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn validate_request_id(value: &str) -> Result<(), ProtocolError> {
    let Some(value) = value.strip_prefix("lfw-xry-") else {
        return Err(ProtocolError::new("gateway request id is invalid"));
    };
    let mut parts = value.split('-');
    let (Some(milliseconds), Some(sequence), None) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(ProtocolError::new("gateway request id is invalid"));
    };
    if is_lower_hex(milliseconds, 16) && is_lower_hex(sequence, 16) {
        Ok(())
    } else {
        Err(ProtocolError::new("gateway request id is invalid"))
    }
}

fn validate_action_contract(
    action: GatewayAction,
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
) -> Result<(), ProtocolError> {
    match action {
        GatewayAction::Progress | GatewayAction::Freeze
            if apply || plan_sha256.is_some() || confirmation_receipt_ref.is_some() =>
        {
            Err(ProtocolError::new("gateway action contract is invalid"))
        }
        GatewayAction::Cleanup | GatewayAction::Archive
            if confirmation_receipt_ref.is_some() || (apply && plan_sha256.is_none()) =>
        {
            Err(ProtocolError::new("gateway action contract is invalid"))
        }
        GatewayAction::Produce
            if apply || plan_sha256.is_some() || confirmation_receipt_ref.is_some() =>
        {
            Err(ProtocolError::new("gateway action contract is invalid"))
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
            Err(ProtocolError::new("gateway action contract is invalid"))
        }
        _ => Ok(()),
    }
}

fn validate_task(task: &str) -> Result<(), ProtocolError> {
    if task.trim() != task {
        return Err(ProtocolError::new("gateway task binding is invalid"));
    }
    let mut segments = task.split('/');
    let (Some(prefix), Some(group), Some(batch), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return Err(ProtocolError::new("gateway task binding is invalid"));
    };
    if prefix == "批量剪辑" && valid_path_segment(group) && valid_path_segment(batch) {
        Ok(())
    } else {
        Err(ProtocolError::new("gateway task binding is invalid"))
    }
}

fn validate_subject(subject: &str) -> Result<(), ProtocolError> {
    let bytes = subject.as_bytes();
    if bytes.len() == 3
        && bytes[0] == b'S'
        && bytes[1..].iter().all(u8::is_ascii_digit)
        && subject != "S00"
    {
        Ok(())
    } else {
        Err(ProtocolError::new("gateway subject binding is invalid"))
    }
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

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests;
