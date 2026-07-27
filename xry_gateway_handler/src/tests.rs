use std::cell::RefCell;

use super::*;
use lightflow_xry_gateway_protocol::{encode_frame, request_sha256_for};
use serde::Serialize;

const REQUEST_ID: &str = "lfw-xry-0000000000000001-0000000000000001";
const TASK: &str = "批量剪辑/皮卡严选 走全球/7.23批量";
const SUBJECT: &str = "S01";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const SHA_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

#[derive(Serialize)]
struct TestRequest<'a> {
    protocol: &'static str,
    request_id: &'a str,
    request_sha256: String,
    action: GatewayAction,
    task: &'a str,
    subject: &'a str,
    apply: bool,
    plan_sha256: Option<&'a str>,
    confirmation_receipt_ref: Option<&'a str>,
}

fn request_frame(action: GatewayAction) -> Vec<u8> {
    request_frame_with(action, false, None, None)
}

fn request_frame_with(
    action: GatewayAction,
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
) -> Vec<u8> {
    let request_sha256 = request_sha256_for(
        REQUEST_ID,
        action,
        TASK,
        SUBJECT,
        apply,
        plan_sha256,
        confirmation_receipt_ref,
    )
    .expect("hash");
    encode_frame(&TestRequest {
        protocol: PROTOCOL_VERSION,
        request_id: REQUEST_ID,
        request_sha256,
        action,
        task: TASK,
        subject: SUBJECT,
        apply,
        plan_sha256,
        confirmation_receipt_ref,
    })
    .expect("frame")
}

fn opaque(hash: &str) -> OpaqueReference {
    OpaqueReference::new(format!("opaque:{hash}")).expect("opaque reference")
}

fn framed_json(frame: &[u8]) -> &str {
    assert!(frame.len() >= 4, "response frame has a header");
    let mut header = [0_u8; 4];
    header.copy_from_slice(&frame[..4]);
    assert_eq!(frame.len(), 4 + u32::from_be_bytes(header) as usize);
    std::str::from_utf8(&frame[4..]).expect("UTF-8 framed JSON payload")
}

fn produce_pass() -> CanonicalPass {
    CanonicalPass::new(
        SHA_A,
        PublicReport::produce(opaque(SHA_B), opaque(SHA_C), opaque(SHA_D)),
    )
    .expect("pass")
}

struct RecordingBackend {
    seen: RefCell<Option<ValidatedRequest>>,
}

impl CanonicalBackend for RecordingBackend {
    fn execute(&self, request: &ValidatedRequest) -> Result<CanonicalPass, BackendError> {
        self.seen.replace(Some(request.clone()));
        Ok(produce_pass())
    }
}

struct RedactionRecordingBackend {
    seen: RefCell<Option<ValidatedRequest>>,
}

impl CanonicalBackend for RedactionRecordingBackend {
    fn execute(&self, request: &ValidatedRequest) -> Result<CanonicalPass, BackendError> {
        self.seen.replace(Some(request.clone()));
        let approval_plan_sha256 = request.plan_sha256().unwrap_or(SHA_B);
        let state = if request.apply() {
            RedactionState::Applied
        } else {
            RedactionState::ReviewRequired
        };
        let report = PublicReport::redact(
            state,
            approval_plan_sha256,
            opaque(SHA_C),
            opaque(SHA_D),
            opaque(SHA_E),
        )
        .map_err(|_| BackendError::non_pass())?;
        CanonicalPass::redact(SHA_A, approval_plan_sha256, report)
            .map_err(|_| BackendError::non_pass())
    }
}

struct LeakyBackend;

impl CanonicalBackend for LeakyBackend {
    fn execute(&self, _request: &ValidatedRequest) -> Result<CanonicalPass, BackendError> {
        Err(BackendError::test_only(
            "private backend failure: hidden value",
        ))
    }
}

struct ControlReportForProduce;

impl CanonicalBackend for ControlReportForProduce {
    fn execute(&self, _request: &ValidatedRequest) -> Result<CanonicalPass, BackendError> {
        CanonicalPass::new(SHA_A, PublicReport::control(opaque(SHA_B)))
            .map_err(|_| BackendError::non_pass())
    }
}

#[test]
fn backend_receives_only_the_validated_typed_tuple() {
    let backend = RecordingBackend {
        seen: RefCell::new(None),
    };
    let input_frame = request_frame(GatewayAction::Produce);
    let mut input = input_frame.as_slice();
    let mut output = Vec::new();

    run_with_backend(&mut input, &mut output, &backend).expect("typed backend PASS");

    let request = backend.seen.borrow().clone().expect("backend saw request");
    assert_eq!(request.request_id(), REQUEST_ID);
    assert_eq!(request.action(), GatewayAction::Produce);
    assert_eq!(request.task(), TASK);
    assert_eq!(request.subject(), SUBJECT);
    assert_eq!(request.plan_sha256(), None);
    assert_eq!(request.confirmation_receipt_ref(), None);
}

#[test]
fn success_response_echoes_the_request_and_contains_only_opaque_report_values() {
    let backend = RecordingBackend {
        seen: RefCell::new(None),
    };
    let input_frame = request_frame(GatewayAction::Produce);
    let mut input = input_frame.as_slice();
    let mut output = Vec::new();

    run_with_backend(&mut input, &mut output, &backend).expect("PASS");

    let response = String::from_utf8(output).expect("UTF-8 framed JSON");
    assert!(response.contains(&format!("\"request_id\":\"{REQUEST_ID}\"")));
    assert!(response.contains(&format!(
            "\"request_sha256\":\"{}\"",
            request_sha256_for(
                REQUEST_ID,
                GatewayAction::Produce,
                TASK,
                SUBJECT,
                false,
                None,
                None,
            )
            .expect("hash")
        )));
    assert!(response.contains("\"action\":\"produce\""));
    assert!(response.contains("\"stage\":\"produce\""));
    assert!(response.contains("\"status\":\"PASS\""));
    assert!(response.contains(&format!("\"opaque_ref\":\"opaque:{SHA_B}\"")));
    assert!(response.contains(&format!("\"task_state_path\":\"opaque:{SHA_D}\"")));
    assert!(!response.contains("private backend failure"));
}

#[test]
fn unavailable_backend_is_static_and_writes_no_stdout() {
    let input_frame = request_frame(GatewayAction::Produce);
    let mut input = input_frame.as_slice();
    let mut output = Vec::new();

    let error = run_with_backend(&mut input, &mut output, &UnavailableBackend)
        .expect_err("unavailable backend must stop");
    assert_eq!(error.to_string(), "canonical backend did not return PASS");
    assert!(output.is_empty());

    let staging_input_frame = request_frame(GatewayAction::Produce);
    let mut staging_input = staging_input_frame.as_slice();
    let staging_error = run_staging(&mut staging_input).expect_err("staging must stop");
    assert_eq!(
        staging_error.to_string(),
        "canonical backend is unavailable"
    );
}

#[test]
fn raw_backend_errors_and_report_kind_mismatches_never_reach_stdout() {
    let input_frame = request_frame(GatewayAction::Produce);
    let mut input = input_frame.as_slice();
    let mut output = Vec::new();
    let error = run_with_backend(&mut input, &mut output, &LeakyBackend)
        .expect_err("backend failure must stop");
    assert_eq!(error.to_string(), "canonical backend did not return PASS");
    assert!(!error.to_string().contains("private backend failure"));
    assert!(output.is_empty());

    let mismatched_input_frame = request_frame(GatewayAction::Produce);
    let mut mismatched_input = mismatched_input_frame.as_slice();
    let mut mismatched_output = Vec::new();
    let mismatched_error = run_with_backend(
        &mut mismatched_input,
        &mut mismatched_output,
        &ControlReportForProduce,
    )
    .expect_err("report action mismatch must stop");
    assert_eq!(
        mismatched_error.to_string(),
        "canonical report does not match the request contract"
    );
    assert!(mismatched_output.is_empty());
}

#[test]
fn opaque_references_and_receipts_reject_noncanonical_text() {
    assert!(OpaqueReference::new("path-like-reference").is_err());
    assert!(CanonicalPass::new("not-a-hash", PublicReport::control(opaque(SHA_A))).is_err());
}

#[test]
fn redaction_handoff_is_typed_and_preserves_preview_approval_and_apply_confirmation() {
    let backend = RedactionRecordingBackend {
        seen: RefCell::new(None),
    };
    let preview_frame = request_frame_with(GatewayAction::Redact, false, None, None);
    let mut preview_input = preview_frame.as_slice();
    let mut preview_output = Vec::new();
    run_with_backend(&mut preview_input, &mut preview_output, &backend).expect("preview PASS");

    let preview_seen = backend.seen.borrow().clone().expect("preview handoff");
    assert_eq!(preview_seen.action(), GatewayAction::Redact);
    assert_eq!(preview_seen.plan_sha256(), None);
    assert_eq!(preview_seen.confirmation_receipt_ref(), None);
    let preview_response = framed_json(&preview_output);
    assert!(preview_response.contains("\"plan_sha256\":null"));
    assert!(preview_response.contains("\"confirmation_receipt_ref\":null"));
    assert!(preview_response.contains(&format!("\"approval_plan_sha256\":\"{SHA_B}\"")));
    assert!(preview_response.contains("\"state\":\"REVIEW_REQUIRED\""));
    assert!(preview_response.contains(REDACTION_POLICY_VERSION));

    let confirmation_receipt_ref = format!("opaque:{SHA_C}");
    let apply_frame = request_frame_with(
        GatewayAction::Redact,
        true,
        Some(SHA_B),
        Some(&confirmation_receipt_ref),
    );
    let mut apply_input = apply_frame.as_slice();
    let mut apply_output = Vec::new();
    run_with_backend(&mut apply_input, &mut apply_output, &backend).expect("apply PASS");

    let apply_seen = backend.seen.borrow().clone().expect("apply handoff");
    assert_eq!(apply_seen.plan_sha256(), Some(SHA_B));
    assert_eq!(
        apply_seen.confirmation_receipt_ref(),
        Some(confirmation_receipt_ref.as_str())
    );
    let apply_response = framed_json(&apply_output);
    assert!(apply_response.contains(&format!(
        "\"confirmation_receipt_ref\":\"{confirmation_receipt_ref}\""
    )));
    assert!(apply_response.contains("\"state\":\"APPLIED\""));
    assert!(!apply_response.contains("private backend failure"));
}
