use super::*;
use crate::protocol_frame::encode_frame;
use serde_json::{Value, json};

const TASK: &str = "批量剪辑/皮卡严选 走全球/7.23批量";
const SUBJECT: &str = "S01";
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OPAQUE_REF: &str = "opaque:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const REVIEW_REF: &str = "opaque:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const PREVIEW_REF: &str = "opaque:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const MODEL_REF: &str = "opaque:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

fn response_for(request: &GatewayRequest) -> Value {
    json!({
        "protocol": PROTOCOL_VERSION,
        "request_id": request.request_id(),
        "request_sha256": request.request_sha256(),
        "action": request.action().as_str(),
        "stage": request.action().as_str(),
        "task": request.task(),
        "subject": request.subject(),
        "apply": request.apply(),
        "plan_sha256": request.plan_sha256(),
        "confirmation_receipt_ref": request.confirmation_receipt_ref(),
        "approval_plan_sha256": null,
        "status": "PASS",
        "gateway_identity": "xry-gateway-test-v1",
        "receipt_sha256": SHA,
        "report": {},
    })
}

#[test]
fn task_and_subject_must_be_exact_bindings() {
    assert!(GatewayRequest::produce(TASK, SUBJECT).is_ok());
    assert!(GatewayRequest::produce("批量剪辑/组", SUBJECT).is_err());
    assert!(GatewayRequest::produce("批量剪辑/../7.23批量", SUBJECT).is_err());
    assert!(GatewayRequest::produce(" 批量剪辑/组/批次", SUBJECT).is_err());
    assert!(GatewayRequest::produce(TASK, "S00").is_err());
    assert!(GatewayRequest::produce(TASK, "S001").is_err());
}

#[test]
fn destructive_control_requires_an_exact_receipt() {
    assert!(GatewayRequest::control(ControlAction::Cleanup, TASK, SUBJECT, true, None,).is_err());
    assert!(
        GatewayRequest::control(
            ControlAction::Archive,
            TASK,
            SUBJECT,
            true,
            Some(SHA.to_owned()),
        )
        .is_ok()
    );
    assert!(
        GatewayRequest::control(
            ControlAction::Progress,
            TASK,
            SUBJECT,
            false,
            Some(SHA.to_owned()),
        )
        .is_err()
    );
}

#[test]
fn redact_request_requires_exact_preview_or_confirmed_apply_and_signs_the_confirmation() {
    assert!(GatewayRequest::redact(TASK, SUBJECT, false, None, None).is_ok());
    assert!(GatewayRequest::redact(TASK, SUBJECT, false, Some(SHA.to_owned()), None).is_err());
    assert!(GatewayRequest::redact(TASK, SUBJECT, true, Some(SHA.to_owned()), None).is_err());
    assert!(
        GatewayRequest::redact(
            TASK,
            SUBJECT,
            true,
            Some(SHA.to_owned()),
            Some("/srv/xry/not-a-receipt".to_owned()),
        )
        .is_err()
    );

    let request = GatewayRequest::redact(
        TASK,
        SUBJECT,
        true,
        Some(SHA.to_owned()),
        Some(OPAQUE_REF.to_owned()),
    )
    .expect("confirmed apply request");
    let exact = UnsignedGatewayRequest {
        protocol: PROTOCOL_VERSION,
        request_id: request.request_id(),
        action: GatewayAction::Redact,
        task: TASK,
        subject: SUBJECT,
        apply: true,
        plan_sha256: Some(SHA),
        confirmation_receipt_ref: Some(OPAQUE_REF),
    };
    assert_eq!(
        crate::protocol_frame::request_sha256(&exact).expect("exact hash"),
        request.request_sha256()
    );
    let missing_confirmation = UnsignedGatewayRequest {
        confirmation_receipt_ref: None,
        ..exact
    };
    assert_ne!(
        crate::protocol_frame::request_sha256(&missing_confirmation).expect("changed hash"),
        request.request_sha256()
    );
}

#[test]
fn response_requires_one_exact_pass_frame() {
    let request = GatewayRequest::produce(TASK, SUBJECT).expect("valid request");
    let response = response_for(&request);
    let frame = encode_frame(&response).expect("response frame");
    let decoded = decode_response_frame(&frame, &request).expect("exact PASS response");
    assert_eq!(decoded.report(), &json!({}));

    let mut unknown = response_for(&request);
    unknown
        .as_object_mut()
        .expect("object")
        .insert("unexpected".to_owned(), Value::Bool(true));
    assert!(decode_response_frame(&encode_frame(&unknown).expect("frame"), &request).is_err());

    let mut trailing = frame;
    trailing.push(0);
    assert!(decode_response_frame(&trailing, &request).is_err());
}

#[test]
fn production_result_needs_all_canonical_fields() {
    let request = GatewayRequest::produce(TASK, SUBJECT).expect("valid request");
    let mut response = response_for(&request);
    response["report"] = json!({
        "worker_context": {"subject": SUBJECT},
        "production_report": {"canonical": true},
        "task_state_path": "/srv/xry/2.预处理/批量剪辑/皮卡严选 走全球/7.23批量/.pipeline/task-state.json",
    });
    let decoded = decode_response_frame(&encode_frame(&response).expect("frame"), &request)
        .expect("response");
    assert_eq!(
        decoded.production_result().expect("result").task_state_path,
        response["report"]["task_state_path"]
    );
}

#[test]
fn redact_response_requires_a_typed_preview_result_and_exact_apply_confirmation_echo() {
    let preview = GatewayRequest::redact(TASK, SUBJECT, false, None, None).expect("preview");
    let mut preview_response = response_for(&preview);
    preview_response["approval_plan_sha256"] = Value::String(SHA.to_owned());
    preview_response["report"] = json!({
        "state": "REVIEW_REQUIRED",
        "policy_version": REDACTION_POLICY_VERSION,
        "approval_plan_sha256": SHA,
        "review_packet_ref": REVIEW_REF,
        "preview_receipt_ref": PREVIEW_REF,
        "model_receipt_ref": MODEL_REF,
    });
    let decoded = decode_response_frame(&encode_frame(&preview_response).expect("frame"), &preview)
        .expect("typed preview response");
    let result = decoded.redaction_result().expect("typed result");
    assert_eq!(result.state(), RedactionState::ReviewRequired);
    assert_eq!(result.approval_plan_sha256(), SHA);
    assert_eq!(result.review_packet_ref().as_str(), REVIEW_REF);

    let mut rejected = preview_response.clone();
    rejected["report"]["error"] = Value::String("/srv/xry/private-error".to_owned());
    assert!(decode_response_frame(&encode_frame(&rejected).expect("frame"), &preview).is_err());

    let apply = GatewayRequest::redact(
        TASK,
        SUBJECT,
        true,
        Some(SHA.to_owned()),
        Some(OPAQUE_REF.to_owned()),
    )
    .expect("apply");
    let mut apply_response = response_for(&apply);
    apply_response["approval_plan_sha256"] = Value::String(SHA.to_owned());
    apply_response["report"] = json!({
        "state": "APPLIED",
        "policy_version": REDACTION_POLICY_VERSION,
        "approval_plan_sha256": SHA,
        "review_packet_ref": REVIEW_REF,
        "preview_receipt_ref": PREVIEW_REF,
        "model_receipt_ref": MODEL_REF,
    });
    assert!(decode_response_frame(&encode_frame(&apply_response).expect("frame"), &apply).is_ok());

    apply_response["confirmation_receipt_ref"] = Value::String(REVIEW_REF.to_owned());
    assert!(decode_response_frame(&encode_frame(&apply_response).expect("frame"), &apply).is_err());
}
