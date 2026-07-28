//! Strict adapter for the generic LightFlow command-process protocol.
//!
//! This module owns only wire parsing and serialization. Route selection stays
//! in the dispatcher so this adapter cannot introduce arbitrary execution.

use lightflow::runner::{
    ModelBinding, PROTOCOL as RUNNER_PROTOCOL, Request, Response, RunnerResult, WorkflowIdentity,
};
use lightflow::serde_json::{self, Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;

pub(crate) const COMMAND_PROTOCOL: &str = "lightflow.command.v1";

pub(crate) struct CommandRequest {
    pub(crate) workflow: WorkflowIdentity,
    pub(crate) inputs: Map<String, Value>,
    pub(crate) models: BTreeMap<String, ModelBinding>,
}

pub(crate) enum ProtocolRequest {
    Runner(Request),
    Command(CommandRequest),
}

pub(crate) fn read_protocol_request(reader: &mut impl Read) -> RunnerResult<ProtocolRequest> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let envelope: Value = serde_json::from_slice(&bytes)?;
    let protocol = envelope_protocol(&envelope)?;

    match protocol.as_str() {
        RUNNER_PROTOCOL => Ok(ProtocolRequest::Runner(serde_json::from_value(envelope)?)),
        COMMAND_PROTOCOL => Ok(ProtocolRequest::Command(parse_command_request(envelope)?)),
        _ => Err(format!("unsupported LightFlow command protocol: {protocol}").into()),
    }
}

pub(crate) fn write_command_response(
    writer: &mut impl Write,
    response: Response,
) -> RunnerResult<()> {
    serde_json::to_writer(&mut *writer, &command_response_value(response)?)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn envelope_protocol(envelope: &Value) -> RunnerResult<String> {
    envelope
        .as_object()
        .and_then(|fields| fields.get("protocol"))
        .and_then(Value::as_str)
        .filter(|protocol| !protocol.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "command request has a missing or invalid protocol".into())
}

fn parse_command_request(envelope: Value) -> RunnerResult<CommandRequest> {
    let Value::Object(mut fields) = envelope else {
        return Err("generic command request must be a JSON object".into());
    };
    reject_unknown_fields(
        &fields,
        &["protocol", "workflow", "inputs", "models"],
        "generic command request",
    )?;
    let protocol = take_required_text(&mut fields, "protocol", "generic command request")?;
    if protocol != COMMAND_PROTOCOL {
        return Err(format!("expected {COMMAND_PROTOCOL}, got {protocol}").into());
    }
    let workflow = parse_command_identity(take_required_value(
        &mut fields,
        "workflow",
        "generic command request",
    )?)?;
    let inputs = take_required_object(&mut fields, "inputs", "generic command request")?;
    let models = parse_models(fields.remove("models"))?;
    debug_assert!(fields.is_empty());
    Ok(CommandRequest {
        workflow,
        inputs,
        models,
    })
}

fn parse_command_identity(value: Value) -> RunnerResult<WorkflowIdentity> {
    let Value::Object(mut fields) = value else {
        return Err("generic command workflow must be a JSON object".into());
    };
    reject_unknown_fields(&fields, &["id", "version"], "generic command workflow")?;
    let id = take_required_text(&mut fields, "id", "generic command workflow")?;
    let version = take_required_text(&mut fields, "version", "generic command workflow")?;
    debug_assert!(fields.is_empty());
    Ok(WorkflowIdentity { id, version })
}

fn parse_models(value: Option<Value>) -> RunnerResult<BTreeMap<String, ModelBinding>> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let Value::Object(bindings) = value else {
        return Err("generic command request has invalid models".into());
    };

    let mut models = BTreeMap::new();
    for (name, binding) in bindings {
        if name.trim().is_empty() {
            return Err("generic command models contains an empty binding name".into());
        }
        models.insert(name, parse_model_binding(binding)?);
    }
    Ok(models)
}

fn parse_model_binding(value: Value) -> RunnerResult<ModelBinding> {
    let Value::Object(mut fields) = value else {
        return Err("generic command model binding must be a JSON object".into());
    };
    reject_unknown_fields(
        &fields,
        &[
            "requirement_id",
            "variant_id",
            "path",
            "sha256",
            "size_bytes",
            "snapshot_revision",
        ],
        "generic command model binding",
    )?;
    let requirement_id = take_required_text(
        &mut fields,
        "requirement_id",
        "generic command model binding",
    )?;
    let variant_id =
        take_required_text(&mut fields, "variant_id", "generic command model binding")?;
    let path = take_required_text(&mut fields, "path", "generic command model binding")?;
    let sha256 = take_optional_text(&mut fields, "sha256", "generic command model binding")?;
    let size_bytes = take_optional_u64(&mut fields, "size_bytes", "generic command model binding")?;
    let snapshot_revision = take_optional_text(
        &mut fields,
        "snapshot_revision",
        "generic command model binding",
    )?;
    debug_assert!(fields.is_empty());
    Ok(ModelBinding {
        requirement_id,
        variant_id,
        path: PathBuf::from(path),
        sha256,
        size_bytes,
        snapshot_revision,
    })
}

fn reject_unknown_fields(
    fields: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> RunnerResult<()> {
    if fields.keys().all(|field| allowed.contains(&field.as_str())) {
        Ok(())
    } else {
        Err(format!("{context} contains an unsupported field").into())
    }
}

fn take_required_value(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> RunnerResult<Value> {
    fields
        .remove(name)
        .ok_or_else(|| format!("{context} has a missing {name}").into())
}

fn take_required_text(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> RunnerResult<String> {
    match take_required_value(fields, name, context)? {
        Value::String(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{context} has an invalid {name}").into()),
    }
}

fn take_required_object(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> RunnerResult<Map<String, Value>> {
    match take_required_value(fields, name, context)? {
        Value::Object(value) => Ok(value),
        _ => Err(format!("{context} has an invalid {name}").into()),
    }
}

fn take_optional_text(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> RunnerResult<Option<String>> {
    match fields.remove(name) {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value)),
        Some(_) => Err(format!("{context} has an invalid {name}").into()),
    }
}

fn take_optional_u64(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> RunnerResult<Option<u64>> {
    match fields.remove(name) {
        None => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{context} has an invalid {name}").into()),
        Some(_) => Err(format!("{context} has an invalid {name}").into()),
    }
}

fn command_response_value(response: Response) -> RunnerResult<Value> {
    let mut fields = Map::from_iter([
        ("outputs".to_owned(), Value::Object(response.outputs)),
        (
            "replay_fingerprint".to_owned(),
            Value::Object(response.replay_fingerprint),
        ),
    ]);
    if !response.artifacts.is_empty() {
        fields.insert(
            "artifacts".to_owned(),
            serde_json::to_value(response.artifacts)?,
        );
    }
    Ok(Value::Object(fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_protocol_stream;
    use lightflow::serde_json::{json, to_vec};
    use std::io::Cursor;

    fn command_request(
        workflow_id: &str,
        workflow_version: &str,
        inputs: Map<String, Value>,
    ) -> Value {
        json!({
            "protocol": COMMAND_PROTOCOL,
            "workflow": {
                "id": workflow_id,
                "version": workflow_version,
            },
            "inputs": inputs,
        })
    }

    #[test]
    fn generic_xry_request_reaches_input_validation_without_gateway_activity() {
        let request = command_request(
            lightflow_xry_batch_control::WORKFLOW_ID,
            lightflow_xry_batch_control::WORKFLOW_VERSION,
            Map::new(),
        );
        let mut reader = Cursor::new(to_vec(&request).expect("request encoding"));
        let mut output = Vec::new();

        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("missing bound inputs must fail before the gateway is invoked");

        assert!(
            error
                .to_string()
                .contains("control request has a missing or invalid required text input")
        );
        assert!(output.is_empty());
    }

    #[test]
    fn generic_command_rejects_non_xry_route_and_wrong_or_unknown_fields() {
        let non_xry = command_request(
            lightflow_video_auto_edit::WORKFLOW_ID,
            lightflow_video_auto_edit::WORKFLOW_VERSION,
            Map::new(),
        );
        let mut reader = Cursor::new(to_vec(&non_xry).expect("request encoding"));
        let mut output = Vec::new();
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("generic command must not expose non-XRY routes");
        assert!(
            error
                .to_string()
                .contains("unsupported generic LightFlow XRY workflow identity")
        );
        assert!(output.is_empty());

        let wrong_protocol = json!({
            "protocol": "lightflow.command.v2",
            "workflow": {
                "id": lightflow_xry_batch_control::WORKFLOW_ID,
                "version": lightflow_xry_batch_control::WORKFLOW_VERSION,
            },
            "inputs": {},
        });
        let mut reader = Cursor::new(to_vec(&wrong_protocol).expect("request encoding"));
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("unknown command protocol must fail before routing");
        assert!(
            error
                .to_string()
                .contains("unsupported LightFlow command protocol")
        );
        assert!(output.is_empty());

        let unknown_field = json!({
            "protocol": COMMAND_PROTOCOL,
            "workflow": {
                "id": lightflow_xry_batch_control::WORKFLOW_ID,
                "version": lightflow_xry_batch_control::WORKFLOW_VERSION,
            },
            "inputs": {},
            "unexpected": true,
        });
        let mut reader = Cursor::new(to_vec(&unknown_field).expect("request encoding"));
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("unknown generic request fields must fail before routing");
        assert!(
            error
                .to_string()
                .contains("generic command request contains an unsupported field")
        );
        assert!(output.is_empty());
    }

    #[test]
    fn generic_command_rejects_malformed_identity_and_inputs_before_routing() {
        let malformed_identity = json!({
            "protocol": COMMAND_PROTOCOL,
            "workflow": {
                "id": lightflow_xry_batch_control::WORKFLOW_ID,
                "version": lightflow_xry_batch_control::WORKFLOW_VERSION,
                "route": "unexpected",
            },
            "inputs": {},
        });
        let mut reader = Cursor::new(to_vec(&malformed_identity).expect("request encoding"));
        let mut output = Vec::new();
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("generic command identity must be strict");
        assert!(
            error
                .to_string()
                .contains("generic command workflow contains an unsupported field")
        );
        assert!(output.is_empty());

        let malformed_inputs = json!({
            "protocol": COMMAND_PROTOCOL,
            "workflow": {
                "id": lightflow_xry_batch_control::WORKFLOW_ID,
                "version": lightflow_xry_batch_control::WORKFLOW_VERSION,
            },
            "inputs": [],
        });
        let mut reader = Cursor::new(to_vec(&malformed_inputs).expect("request encoding"));
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("generic command inputs must be an object");
        assert!(
            error
                .to_string()
                .contains("generic command request has an invalid inputs")
        );
        assert!(output.is_empty());
    }

    #[test]
    fn generic_command_rejects_malformed_model_bindings_before_route_execution() {
        let mut request = command_request(
            lightflow_xry_batch_control::WORKFLOW_ID,
            lightflow_xry_batch_control::WORKFLOW_VERSION,
            Map::new(),
        );
        request.as_object_mut().expect("request object").insert(
            "models".to_owned(),
            json!({
                "detector": {
                    "requirement_id": "grounded-sam2",
                    "variant_id": "base-plus",
                    "path": "/models/grounded-sam2",
                    "unexpected": true,
                }
            }),
        );
        let mut reader = Cursor::new(to_vec(&request).expect("request encoding"));
        let mut output = Vec::new();
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("unknown model fields must fail before route execution");
        assert!(
            error
                .to_string()
                .contains("generic command model binding contains an unsupported field")
        );
        assert!(output.is_empty());

        let mut request = command_request(
            lightflow_xry_batch_produce::WORKFLOW_ID,
            lightflow_xry_batch_produce::WORKFLOW_VERSION,
            Map::new(),
        );
        request.as_object_mut().expect("request object").insert(
            "models".to_owned(),
            json!({
                "detector": {
                    "requirement_id": "grounded-sam2",
                    "variant_id": "base-plus",
                }
            }),
        );
        let mut reader = Cursor::new(to_vec(&request).expect("request encoding"));
        let error = run_protocol_stream(&mut reader, &mut output)
            .expect_err("missing model path must fail before route execution");
        assert!(
            error
                .to_string()
                .contains("generic command model binding has a missing path")
        );
        assert!(output.is_empty());
    }

    #[test]
    fn generic_response_conversion_has_only_command_protocol_fields() {
        let response = Response {
            outputs: Map::from_iter([("summary".to_owned(), json!("PASS"))]),
            artifacts: Vec::new(),
            replay_fingerprint: Map::from_iter([("implementation".to_owned(), json!("test"))]),
        };
        let mut output = Vec::new();

        write_command_response(&mut output, response).expect("response encoding");
        let Value::Object(response) =
            serde_json::from_slice(&output).expect("generic response decoding")
        else {
            panic!("generic response must be an object");
        };

        assert_eq!(response.len(), 2);
        assert_eq!(response.get("outputs"), Some(&json!({"summary": "PASS"})));
        assert_eq!(
            response.get("replay_fingerprint"),
            Some(&json!({"implementation": "test"}))
        );
        assert!(!response.contains_key("artifacts"));
    }
}
