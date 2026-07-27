//! A non-routable, fail-closed staging handler for one XRY gateway v1 request.
//!
//! The production runtime intentionally has no backend integration. Its only runtime backend
//! always returns a static non-PASS result and the binary never opens stdout. The generic
//! success path exists solely for typed tests and a future protected canonical integration.

use std::convert::Infallible;
use std::fmt;
use std::io::{Read, Write};

use lightflow_xry_gateway_protocol::{
    GatewayAction, PROTOCOL_VERSION, ProtocolError, ValidatedRequest, encode_frame,
    read_and_validate_request, validate_sha256, write_frame,
};
use serde::Serialize;

pub const GATEWAY_IDENTITY: &str = "lightflow-xry-gateway-handler-staging-v1";
pub const REDACTION_POLICY_VERSION: &str = "xry-person-redaction.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedactionState {
    ReviewRequired,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandlerError(&'static str);

impl HandlerError {
    const fn new(message: &'static str) -> Self {
        Self(message)
    }
}

impl fmt::Display for HandlerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for HandlerError {}

/// Static backend failure categories. No backend-provided text is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendError(#[allow(dead_code)] &'static str);

impl BackendError {
    pub const fn unavailable() -> Self {
        Self("canonical backend is unavailable")
    }

    pub const fn non_pass() -> Self {
        Self("canonical backend did not return PASS")
    }

    #[cfg(test)]
    const fn test_only(message: &'static str) -> Self {
        Self(message)
    }
}

/// The fixed future integration boundary: a backend receives only a validated tuple.
pub trait CanonicalBackend {
    fn execute(&self, request: &ValidatedRequest) -> Result<CanonicalPass, BackendError>;
}

/// The only runtime backend compiled into this staging artifact.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBackend;

impl CanonicalBackend for UnavailableBackend {
    fn execute(&self, _request: &ValidatedRequest) -> Result<CanonicalPass, BackendError> {
        Err(BackendError::unavailable())
    }
}

/// A receipt-bearing success supplied by a future canonical backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalPass {
    receipt_sha256: String,
    approval_plan_sha256: Option<String>,
    report: PublicReport,
}

impl CanonicalPass {
    pub fn new(
        receipt_sha256: impl Into<String>,
        report: PublicReport,
    ) -> Result<Self, HandlerError> {
        if report.redaction_approval_plan_sha256().is_some() {
            return Err(HandlerError::new(
                "redaction canonical PASS requires an approval plan",
            ));
        }
        Self::with_approval(receipt_sha256.into(), None, report)
    }

    pub fn redact(
        receipt_sha256: impl Into<String>,
        approval_plan_sha256: impl Into<String>,
        report: PublicReport,
    ) -> Result<Self, HandlerError> {
        let approval_plan_sha256 = approval_plan_sha256.into();
        validate_sha256(&approval_plan_sha256)
            .map_err(|_| HandlerError::new("redaction approval hash is invalid"))?;
        if report.redaction_approval_plan_sha256() != Some(approval_plan_sha256.as_str()) {
            return Err(HandlerError::new(
                "redaction report approval hash is invalid",
            ));
        }
        Self::with_approval(receipt_sha256.into(), Some(approval_plan_sha256), report)
    }

    fn with_approval(
        receipt_sha256: String,
        approval_plan_sha256: Option<String>,
        report: PublicReport,
    ) -> Result<Self, HandlerError> {
        validate_sha256(&receipt_sha256)
            .map_err(|_| HandlerError::new("canonical receipt hash is invalid"))?;
        Ok(Self {
            receipt_sha256,
            approval_plan_sha256,
            report,
        })
    }

    fn receipt_sha256(&self) -> &str {
        &self.receipt_sha256
    }

    fn report(&self) -> &PublicReport {
        &self.report
    }

    fn approval_plan_sha256(&self) -> Option<&str> {
        self.approval_plan_sha256.as_deref()
    }

    fn matches_request(&self, request: &ValidatedRequest) -> bool {
        self.report
            .matches_request(request, self.approval_plan_sha256())
    }
}

/// An allowlisted opaque reference, not a path, command, error, or free-form report value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueReference(String);

impl OpaqueReference {
    pub fn new(value: impl Into<String>) -> Result<Self, HandlerError> {
        let value = value.into();
        let Some(hash) = value.strip_prefix("opaque:") else {
            return Err(HandlerError::new("opaque reference is invalid"));
        };
        validate_sha256(hash).map_err(|_| HandlerError::new("opaque reference is invalid"))?;
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Typed public reports deliberately contain references only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicReport {
    Control {
        report_ref: OpaqueReference,
    },
    Produce {
        worker_context_ref: OpaqueReference,
        production_report_ref: OpaqueReference,
        task_state_ref: OpaqueReference,
    },
    Redact {
        state: RedactionState,
        approval_plan_sha256: String,
        review_packet_ref: OpaqueReference,
        preview_receipt_ref: OpaqueReference,
        model_receipt_ref: OpaqueReference,
    },
}

impl PublicReport {
    pub fn control(report_ref: OpaqueReference) -> Self {
        Self::Control { report_ref }
    }

    pub fn produce(
        worker_context_ref: OpaqueReference,
        production_report_ref: OpaqueReference,
        task_state_ref: OpaqueReference,
    ) -> Self {
        Self::Produce {
            worker_context_ref,
            production_report_ref,
            task_state_ref,
        }
    }

    pub fn redact(
        state: RedactionState,
        approval_plan_sha256: impl Into<String>,
        review_packet_ref: OpaqueReference,
        preview_receipt_ref: OpaqueReference,
        model_receipt_ref: OpaqueReference,
    ) -> Result<Self, HandlerError> {
        let approval_plan_sha256 = approval_plan_sha256.into();
        validate_sha256(&approval_plan_sha256)
            .map_err(|_| HandlerError::new("redaction approval hash is invalid"))?;
        Ok(Self::Redact {
            state,
            approval_plan_sha256,
            review_packet_ref,
            preview_receipt_ref,
            model_receipt_ref,
        })
    }

    fn matches_request(
        &self,
        request: &ValidatedRequest,
        approval_plan_sha256: Option<&str>,
    ) -> bool {
        match (self, request.action()) {
            (
                Self::Control { .. },
                GatewayAction::Progress
                | GatewayAction::Freeze
                | GatewayAction::Cleanup
                | GatewayAction::Archive,
            ) => approval_plan_sha256.is_none(),
            (Self::Produce { .. }, GatewayAction::Produce) => approval_plan_sha256.is_none(),
            (
                Self::Redact {
                    state,
                    approval_plan_sha256: report_approval_plan_sha256,
                    ..
                },
                GatewayAction::Redact,
            ) => match (
                request.apply(),
                request.plan_sha256(),
                request.confirmation_receipt_ref(),
                approval_plan_sha256,
            ) {
                (false, None, None, Some(approval_plan_sha256)) => {
                    *state == RedactionState::ReviewRequired
                        && report_approval_plan_sha256.as_str() == approval_plan_sha256
                }
                (true, Some(plan_sha256), Some(_), Some(approval_plan_sha256)) => {
                    *state == RedactionState::Applied
                        && plan_sha256 == approval_plan_sha256
                        && report_approval_plan_sha256.as_str() == approval_plan_sha256
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn as_wire(&self) -> WireReport<'_> {
        match self {
            Self::Control { report_ref } => WireReport::Control {
                report_ref: report_ref.as_str(),
            },
            Self::Produce {
                worker_context_ref,
                production_report_ref,
                task_state_ref,
            } => WireReport::Produce {
                worker_context: OpaqueObject {
                    opaque_ref: worker_context_ref.as_str(),
                },
                production_report: OpaqueObject {
                    opaque_ref: production_report_ref.as_str(),
                },
                task_state_path: task_state_ref.as_str(),
            },
            Self::Redact {
                state,
                approval_plan_sha256,
                review_packet_ref,
                preview_receipt_ref,
                model_receipt_ref,
            } => WireReport::Redact {
                state: *state,
                policy_version: REDACTION_POLICY_VERSION,
                approval_plan_sha256,
                review_packet_ref: review_packet_ref.as_str(),
                preview_receipt_ref: preview_receipt_ref.as_str(),
                model_receipt_ref: model_receipt_ref.as_str(),
            },
        }
    }

    fn redaction_approval_plan_sha256(&self) -> Option<&str> {
        match self {
            Self::Redact {
                approval_plan_sha256,
                ..
            } => Some(approval_plan_sha256),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct PassEnvelope<'a> {
    protocol: &'static str,
    request_id: &'a str,
    request_sha256: &'a str,
    action: GatewayAction,
    stage: GatewayAction,
    task: &'a str,
    subject: &'a str,
    apply: bool,
    plan_sha256: Option<&'a str>,
    confirmation_receipt_ref: Option<&'a str>,
    approval_plan_sha256: Option<&'a str>,
    status: &'static str,
    gateway_identity: &'static str,
    receipt_sha256: &'a str,
    report: WireReport<'a>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum WireReport<'a> {
    Control {
        report_ref: &'a str,
    },
    Produce {
        worker_context: OpaqueObject<'a>,
        production_report: OpaqueObject<'a>,
        task_state_path: &'a str,
    },
    Redact {
        state: RedactionState,
        policy_version: &'static str,
        approval_plan_sha256: &'a str,
        review_packet_ref: &'a str,
        preview_receipt_ref: &'a str,
        model_receipt_ref: &'a str,
    },
}

#[derive(Serialize)]
struct OpaqueObject<'a> {
    opaque_ref: &'a str,
}

/// Run a future typed backend integration. This is not used by the staging binary.
pub fn run_with_backend(
    reader: &mut impl Read,
    writer: &mut impl Write,
    backend: &impl CanonicalBackend,
) -> Result<(), HandlerError> {
    let request = read_and_validate_request(reader)
        .map_err(|_: ProtocolError| HandlerError::new("gateway request rejected"))?;
    let pass = backend
        .execute(&request)
        .map_err(|_| HandlerError::new("canonical backend did not return PASS"))?;
    if !pass.matches_request(&request) {
        return Err(HandlerError::new(
            "canonical report does not match the request contract",
        ));
    }
    let envelope = PassEnvelope {
        protocol: PROTOCOL_VERSION,
        request_id: request.request_id(),
        request_sha256: request.request_sha256(),
        action: request.action(),
        stage: request.action(),
        task: request.task(),
        subject: request.subject(),
        apply: request.apply(),
        plan_sha256: request.plan_sha256(),
        confirmation_receipt_ref: request.confirmation_receipt_ref(),
        approval_plan_sha256: pass.approval_plan_sha256(),
        status: "PASS",
        gateway_identity: GATEWAY_IDENTITY,
        receipt_sha256: pass.receipt_sha256(),
        report: pass.report().as_wire(),
    };
    let frame = encode_frame(&envelope)
        .map_err(|_| HandlerError::new("gateway PASS response could not be encoded"))?;
    write_frame(writer, &frame)
        .map_err(|_| HandlerError::new("gateway PASS response could not be written"))
}

/// Run the only shipped runtime configuration.
///
/// This function never writes stdout and never returns a PASS. It proves framing rejection and
/// preserves the future backend seam without routing a request anywhere.
pub fn run_staging(reader: &mut impl Read) -> Result<Infallible, HandlerError> {
    let request = read_and_validate_request(reader)
        .map_err(|_: ProtocolError| HandlerError::new("gateway request rejected"))?;
    match UnavailableBackend.execute(&request) {
        Ok(_) => Err(HandlerError::new("staging backend must not return PASS")),
        Err(_) => Err(HandlerError::new("canonical backend is unavailable")),
    }
}

#[cfg(test)]
mod tests;
