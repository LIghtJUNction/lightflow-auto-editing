//! Closed in-process dispatch for the public auto-editing command workflows.
//!
//! This crate is the deployment-owned target for `LIGHTFLOW_COMMAND_RUNNER`.
//! It accepts the shared `lightflow.runner.v1` request envelope and can only
//! route to the libraries compiled into this executable.

use lightflow::runner::{
    Request, Response, RunnerResult, read_request, read_request_from_stdin, write_response,
    write_response_to_stdout,
};
use std::io::{Read, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    AutoEdit,
    AutoEditPlan,
    CoverImage,
    RenderEdit,
    Subtitles,
    VideoDescription,
    VideoHighlights,
    VideoWorkSpeech,
    VideoWorkSubtitles,
    VideoWorkVoiceProfile,
    XryBatchControl,
    XryBatchProduce,
    XryPrivacyRedaction,
}

impl Route {
    const ALL: [Self; 13] = [
        Self::AutoEdit,
        Self::AutoEditPlan,
        Self::CoverImage,
        Self::RenderEdit,
        Self::Subtitles,
        Self::VideoDescription,
        Self::VideoHighlights,
        Self::VideoWorkSpeech,
        Self::VideoWorkSubtitles,
        Self::VideoWorkVoiceProfile,
        Self::XryBatchControl,
        Self::XryBatchProduce,
        Self::XryPrivacyRedaction,
    ];

    const fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::AutoEdit => (
                lightflow_video_auto_edit::WORKFLOW_ID,
                lightflow_video_auto_edit::WORKFLOW_VERSION,
            ),
            Self::AutoEditPlan => (
                lightflow_video_auto_edit_plan::WORKFLOW_ID,
                lightflow_video_auto_edit_plan::WORKFLOW_VERSION,
            ),
            Self::CoverImage => (
                lightflow_video_cover_image::WORKFLOW_ID,
                lightflow_video_cover_image::WORKFLOW_VERSION,
            ),
            Self::RenderEdit => (
                lightflow_video_render_edit::WORKFLOW_ID,
                lightflow_video_render_edit::WORKFLOW_VERSION,
            ),
            Self::Subtitles => (
                lightflow_video_subtitles::WORKFLOW_ID,
                lightflow_video_subtitles::WORKFLOW_VERSION,
            ),
            Self::VideoDescription => (
                lightflow_video_description::WORKFLOW_ID,
                lightflow_video_description::WORKFLOW_VERSION,
            ),
            Self::VideoHighlights => (
                lightflow_video_highlights::WORKFLOW_ID,
                lightflow_video_highlights::WORKFLOW_VERSION,
            ),
            Self::VideoWorkSpeech => (
                lightflow_video_work_speech::WORKFLOW_ID,
                lightflow_video_work_speech::WORKFLOW_VERSION,
            ),
            Self::VideoWorkSubtitles => (
                lightflow_video_work_subtitles::WORKFLOW_ID,
                lightflow_video_work_subtitles::WORKFLOW_VERSION,
            ),
            Self::VideoWorkVoiceProfile => (
                lightflow_video_work_voice_profile::WORKFLOW_ID,
                lightflow_video_work_voice_profile::WORKFLOW_VERSION,
            ),
            Self::XryBatchControl => (
                lightflow_xry_batch_control::WORKFLOW_ID,
                lightflow_xry_batch_control::WORKFLOW_VERSION,
            ),
            Self::XryBatchProduce => (
                lightflow_xry_batch_produce::WORKFLOW_ID,
                lightflow_xry_batch_produce::WORKFLOW_VERSION,
            ),
            Self::XryPrivacyRedaction => (
                lightflow_xry_privacy_redaction::WORKFLOW_ID,
                lightflow_xry_privacy_redaction::WORKFLOW_VERSION,
            ),
        }
    }

    fn execute(self, request: &Request) -> RunnerResult<Response> {
        let (workflow_id, workflow_version) = self.identity();
        request.validate_for(workflow_id, workflow_version)?;

        match self {
            Self::AutoEdit => Ok(lightflow_video_auto_edit::execute(&request.inputs)?),
            Self::AutoEditPlan => Ok(lightflow_video_auto_edit_plan::execute(&request.inputs)?),
            Self::CoverImage => Ok(lightflow_video_cover_image::execute(&request.inputs)?),
            Self::RenderEdit => Ok(lightflow_video_render_edit::execute(&request.inputs)?),
            Self::Subtitles => Ok(lightflow_video_subtitles::execute(&request.inputs)?),
            Self::VideoDescription => Ok(lightflow_video_description::execute(&request.inputs)?),
            Self::VideoHighlights => Ok(lightflow_video_highlights::execute(&request.inputs)?),
            Self::VideoWorkSpeech => Ok(lightflow_video_work_speech::execute(&request.inputs)?),
            Self::VideoWorkSubtitles => {
                Ok(lightflow_video_work_subtitles::execute(&request.inputs)?)
            }
            Self::VideoWorkVoiceProfile => Ok(lightflow_video_work_voice_profile::execute(
                &request.inputs,
            )?),
            Self::XryBatchControl => Ok(lightflow_xry_batch_control::execute(&request.inputs)?),
            Self::XryBatchProduce => Ok(lightflow_xry_batch_produce::execute(&request.inputs)?),
            Self::XryPrivacyRedaction => {
                Ok(lightflow_xry_privacy_redaction::execute(&request.inputs)?)
            }
        }
    }
}

/// Dispatch one shared runner request to an exactly matched public workflow.
///
/// # Errors
///
/// Returns an error when the request identity is not in the closed route table,
/// the exact identity validation fails, or the selected workflow rejects input.
pub fn dispatch(request: &Request) -> RunnerResult<Response> {
    request.validate_protocol()?;
    let route = Route::ALL
        .iter()
        .copied()
        .find(|route| {
            let (workflow_id, workflow_version) = route.identity();
            request.workflow.id == workflow_id && request.workflow.version == workflow_version
        })
        .ok_or("unsupported LightFlow auto-editing command workflow identity")?;
    route.execute(request)
}

/// Read, route, and write one exchange using the shared runner protocol helpers.
///
/// # Errors
///
/// Returns an error when the request is malformed, unsupported, rejected by its
/// selected workflow, or its response cannot be written.
pub fn run_stream(reader: &mut impl Read, writer: &mut impl Write) -> RunnerResult<()> {
    let request = read_request(reader)?;
    let response = dispatch(&request)?;
    write_response(writer, &response)?;
    Ok(())
}

/// Run one command exchange over standard input and output.
///
/// # Errors
///
/// Returns an error when the shared command request cannot be read, is not an
/// allow-listed workflow identity, is rejected by that workflow, or its response
/// cannot be written to standard output.
pub fn run_from_stdio() -> RunnerResult<()> {
    let request = read_request_from_stdin()?;
    let response = dispatch(&request)?;
    write_response_to_stdout(&response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightflow::runner::{PROTOCOL, WorkflowIdentity};
    use lightflow::serde_json::{Map, Value, to_vec};
    use std::collections::BTreeMap;
    use std::io::Cursor;

    fn request(workflow_id: &str, workflow_version: &str, inputs: Map<String, Value>) -> Request {
        Request {
            protocol: PROTOCOL.to_owned(),
            workflow: WorkflowIdentity {
                id: workflow_id.to_owned(),
                version: workflow_version.to_owned(),
            },
            inputs,
            models: BTreeMap::new(),
        }
    }

    #[test]
    fn route_table_covers_every_public_command_workflow_once() {
        let identities = Route::ALL
            .iter()
            .map(|route| route.identity())
            .collect::<Vec<_>>();
        assert_eq!(
            identities,
            vec![
                ("lightflow.video_auto_edit", "0.1.0"),
                ("lightflow.video_auto_edit_plan", "0.1.0"),
                ("lightflow.video_cover_image", "0.1.0"),
                ("lightflow.video_render_edit", "0.1.0"),
                ("lightflow.video_subtitles", "0.1.0"),
                ("lightflow.video_description", "0.1.0"),
                ("lightflow.video_highlights", "0.1.0"),
                ("lightflow.video_work_speech", "0.1.0"),
                ("lightflow.video_work_subtitles", "0.1.0"),
                ("lightflow.video_work_voice_profile", "0.1.0"),
                ("lightflow.xry_batch_control", "0.1.0"),
                ("lightflow.xry_batch_produce", "0.1.0"),
                ("lightflow.xry_privacy_redaction", "0.1.0"),
            ]
        );
    }

    #[test]
    fn dispatch_rejects_an_unknown_workflow_identity() {
        let request = request("lightflow.unknown", "0.1.0", Map::new());
        let error = dispatch(&request).expect_err("unknown route must fail closed");
        assert!(
            error
                .to_string()
                .contains("unsupported LightFlow auto-editing command workflow identity")
        );
    }

    #[test]
    fn dispatch_rejects_a_known_workflow_with_a_mismatched_version() {
        let request = request("lightflow.video_auto_edit", "9.9.9", Map::new());
        let error = dispatch(&request).expect_err("mismatched version must fail closed");
        assert!(
            error
                .to_string()
                .contains("unsupported LightFlow auto-editing command workflow identity")
        );
    }

    #[test]
    fn stream_dispatch_reaches_in_process_workflow_validation_without_external_execution() {
        let request = request(
            lightflow_video_auto_edit_plan::WORKFLOW_ID,
            lightflow_video_auto_edit_plan::WORKFLOW_VERSION,
            Map::new(),
        );
        let mut reader = Cursor::new(to_vec(&request).expect("request encoding"));
        let mut output = Vec::new();

        let error = run_stream(&mut reader, &mut output)
            .expect_err("missing plan inputs must be rejected before external execution");

        assert!(error.to_string().contains("clips"));
        assert!(output.is_empty());
    }

    #[test]
    fn stream_dispatch_rejects_an_unknown_identity_before_writing_stdout() {
        let request = request("lightflow.unknown", "0.1.0", Map::new());
        let mut reader = Cursor::new(to_vec(&request).expect("request encoding"));
        let mut output = Vec::new();

        let error = run_stream(&mut reader, &mut output)
            .expect_err("unknown workflow must be rejected before output");

        assert!(error.to_string().contains("unsupported LightFlow"));
        assert!(output.is_empty());
    }

    #[test]
    fn dispatch_rejects_a_non_runner_protocol_before_routing() {
        let mut request = request(
            lightflow_xry_privacy_redaction::WORKFLOW_ID,
            lightflow_xry_privacy_redaction::WORKFLOW_VERSION,
            Map::new(),
        );
        request.protocol = "lightflow.command.v1".to_owned();

        let error = dispatch(&request).expect_err("wrong protocol must fail closed");
        assert!(error.to_string().contains(PROTOCOL));
    }
}
