//! Public, fail-closed client for the canonical XRY gateway subsystem.
//!
//! This crate deliberately owns only a fixed, framed protocol and a locked-down
//! transport. It has no XRY command strings, file-path inputs, or shell fallback.

mod protocol;
mod protocol_frame;
mod protocol_response;
mod transport;

pub use protocol::{
    ControlAction, GatewayAction, GatewayError, GatewayRequest, GatewayResponse, OpaqueReference,
    PROTOCOL_VERSION, ProductionResult, REDACTION_POLICY_VERSION, RedactionResult, RedactionState,
    SUBSYSTEM_NAME,
};
pub use transport::{invoke, trusted_transport_ready};
