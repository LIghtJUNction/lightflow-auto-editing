use super::*;

const REQUEST_ID: &str = "lfw-xry-0000000000000001-0000000000000001";
const TASK: &str = "批量剪辑/皮卡严选 走全球/7.23批量";
const SUBJECT: &str = "S01";
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OPAQUE_REF: &str = "opaque:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

#[derive(Serialize)]
struct TestRequestWithUnknown<'a> {
    protocol: &'static str,
    request_id: &'a str,
    request_sha256: String,
    action: GatewayAction,
    task: &'a str,
    subject: &'a str,
    apply: bool,
    plan_sha256: Option<&'a str>,
    confirmation_receipt_ref: Option<&'a str>,
    unexpected: bool,
}

fn request_frame(
    action: GatewayAction,
    task: &str,
    subject: &str,
    apply: bool,
    plan_sha256: Option<&str>,
) -> Vec<u8> {
    request_frame_with_hash(action, task, subject, apply, plan_sha256, None, None)
}

fn redact_frame(
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
) -> Vec<u8> {
    request_frame_with_hash(
        GatewayAction::Redact,
        TASK,
        SUBJECT,
        apply,
        plan_sha256,
        confirmation_receipt_ref,
        None,
    )
}

fn request_frame_with_hash(
    action: GatewayAction,
    task: &str,
    subject: &str,
    apply: bool,
    plan_sha256: Option<&str>,
    confirmation_receipt_ref: Option<&str>,
    hash_override: Option<&str>,
) -> Vec<u8> {
    let request_sha256 = hash_override.map_or_else(
        || {
            request_sha256_for(
                REQUEST_ID,
                action,
                task,
                subject,
                apply,
                plan_sha256,
                confirmation_receipt_ref,
            )
            .expect("hash")
        },
        ToOwned::to_owned,
    );
    encode_frame(&TestRequest {
        protocol: PROTOCOL_VERSION,
        request_id: REQUEST_ID,
        request_sha256,
        action,
        task,
        subject,
        apply,
        plan_sha256,
        confirmation_receipt_ref,
    })
    .expect("frame")
}

#[test]
fn accepts_one_exact_valid_frame() {
    let frame = request_frame(GatewayAction::Produce, TASK, SUBJECT, false, None);
    let request = decode_and_validate_request_frame(&frame).expect("valid request");

    assert_eq!(request.request_id(), REQUEST_ID);
    assert_eq!(request.action(), GatewayAction::Produce);
    assert_eq!(request.task(), TASK);
    assert_eq!(request.subject(), SUBJECT);
    assert!(!request.apply());
    assert_eq!(request.plan_sha256(), None);
    assert_eq!(request.confirmation_receipt_ref(), None);
}

#[test]
fn rejects_malformed_trailing_and_oversized_frames() {
    assert!(decode_and_validate_request_frame(&[]).is_err());

    let mut trailing = request_frame(GatewayAction::Produce, TASK, SUBJECT, false, None);
    trailing.push(0);
    assert!(decode_and_validate_request_frame(&trailing).is_err());

    let oversized = ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes();
    assert!(decode_and_validate_request_frame(&oversized).is_err());
}

#[test]
fn rejects_unknown_fields_and_bad_request_hashes() {
    let request_sha256 = request_sha256_for(
        REQUEST_ID,
        GatewayAction::Produce,
        TASK,
        SUBJECT,
        false,
        None,
        None,
    )
    .expect("hash");
    let unknown = encode_frame(&TestRequestWithUnknown {
        protocol: PROTOCOL_VERSION,
        request_id: REQUEST_ID,
        request_sha256,
        action: GatewayAction::Produce,
        task: TASK,
        subject: SUBJECT,
        apply: false,
        plan_sha256: None,
        confirmation_receipt_ref: None,
        unexpected: true,
    })
    .expect("frame");
    assert!(decode_and_validate_request_frame(&unknown).is_err());

    let bad_hash = request_frame_with_hash(
        GatewayAction::Produce,
        TASK,
        SUBJECT,
        false,
        None,
        None,
        Some(SHA),
    );
    assert!(decode_and_validate_request_frame(&bad_hash).is_err());
}

#[test]
fn rejects_invalid_task_subject_action_apply_and_plan_contracts() {
    assert!(
        decode_and_validate_request_frame(&request_frame(
            GatewayAction::Produce,
            "批量剪辑/../7.23批量",
            SUBJECT,
            false,
            None,
        ))
        .is_err()
    );
    assert!(
        decode_and_validate_request_frame(&request_frame(
            GatewayAction::Produce,
            TASK,
            "S00",
            false,
            None,
        ))
        .is_err()
    );
    assert!(
        decode_and_validate_request_frame(&request_frame(
            GatewayAction::Progress,
            TASK,
            SUBJECT,
            true,
            None,
        ))
        .is_err()
    );
    assert!(
        decode_and_validate_request_frame(&request_frame(
            GatewayAction::Cleanup,
            TASK,
            SUBJECT,
            true,
            None,
        ))
        .is_err()
    );
    assert!(
        decode_and_validate_request_frame(&request_frame(
            GatewayAction::Produce,
            TASK,
            SUBJECT,
            false,
            Some(SHA),
        ))
        .is_err()
    );

    assert!(decode_and_validate_request_frame(&redact_frame(false, Some(SHA), None,)).is_err());
    assert!(decode_and_validate_request_frame(&redact_frame(true, Some(SHA), None,)).is_err());
    assert!(
        decode_and_validate_request_frame(&request_frame_with_hash(
            GatewayAction::Produce,
            TASK,
            SUBJECT,
            false,
            None,
            Some(OPAQUE_REF),
            None,
        ))
        .is_err()
    );

    let invalid_action_payload = format!(
        "{{\"protocol\":\"{PROTOCOL_VERSION}\",\"request_id\":\"{REQUEST_ID}\",\"request_sha256\":\"{SHA}\",\"action\":\"other\",\"task\":\"{TASK}\",\"subject\":\"{SUBJECT}\",\"apply\":false,\"plan_sha256\":null,\"confirmation_receipt_ref\":null}}"
    );
    assert!(
        decode_and_validate_request_frame(&encode_raw_payload(invalid_action_payload.as_bytes()))
            .is_err()
    );
}

#[test]
fn requires_an_explicit_nullable_plan_field() {
    let payload = format!(
        "{{\"protocol\":\"{PROTOCOL_VERSION}\",\"request_id\":\"{REQUEST_ID}\",\"request_sha256\":\"{SHA}\",\"action\":\"produce\",\"task\":\"{TASK}\",\"subject\":\"{SUBJECT}\",\"apply\":false,\"confirmation_receipt_ref\":null}}"
    );
    assert!(decode_and_validate_request_frame(&encode_raw_payload(payload.as_bytes())).is_err());

    let payload = format!(
        "{{\"protocol\":\"{PROTOCOL_VERSION}\",\"request_id\":\"{REQUEST_ID}\",\"request_sha256\":\"{SHA}\",\"action\":\"produce\",\"task\":\"{TASK}\",\"subject\":\"{SUBJECT}\",\"apply\":false,\"plan_sha256\":null}}"
    );
    assert!(decode_and_validate_request_frame(&encode_raw_payload(payload.as_bytes())).is_err());
}

#[test]
fn redaction_requires_signed_preview_or_confirmed_apply() {
    let preview = redact_frame(false, None, None);
    let preview_request = decode_and_validate_request_frame(&preview).expect("preview");
    assert_eq!(preview_request.action(), GatewayAction::Redact);
    assert_eq!(preview_request.plan_sha256(), None);
    assert_eq!(preview_request.confirmation_receipt_ref(), None);

    let apply = redact_frame(true, Some(SHA), Some(OPAQUE_REF));
    let apply_request = decode_and_validate_request_frame(&apply).expect("apply");
    assert_eq!(apply_request.plan_sha256(), Some(SHA));
    assert_eq!(apply_request.confirmation_receipt_ref(), Some(OPAQUE_REF));

    let different_confirmation_hash = request_sha256_for(
        REQUEST_ID,
        GatewayAction::Redact,
        TASK,
        SUBJECT,
        true,
        Some(SHA),
        None,
    )
    .expect("hash");
    let mismatched = encode_frame(&TestRequest {
        protocol: PROTOCOL_VERSION,
        request_id: REQUEST_ID,
        request_sha256: different_confirmation_hash,
        action: GatewayAction::Redact,
        task: TASK,
        subject: SUBJECT,
        apply: true,
        plan_sha256: Some(SHA),
        confirmation_receipt_ref: Some(OPAQUE_REF),
    })
    .expect("frame");
    assert!(decode_and_validate_request_frame(&mismatched).is_err());
}

fn encode_raw_payload(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}
