use super::*;
use crate::protocol_frame::encode_frame;
use serde_json::{Value, json};

const TASK: &str = "批量剪辑/皮卡严选 走全球/7.23批量";
const SUBJECT: &str = "S01";
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

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
