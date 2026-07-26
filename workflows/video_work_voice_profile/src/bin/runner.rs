fn main() -> std::process::ExitCode {
    run().map_or_else(
        |error| {
            eprintln!("Video Work voice-profile runner: {error}");
            std::process::ExitCode::FAILURE
        },
        |()| std::process::ExitCode::SUCCESS,
    )
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request = lightflow::runner::read_request_from_stdin()?;
    request.validate_for(
        lightflow_video_work_voice_profile::WORKFLOW_ID,
        lightflow_video_work_voice_profile::WORKFLOW_VERSION,
    )?;
    lightflow::runner::write_response_to_stdout(&lightflow_video_work_voice_profile::execute(
        &request.inputs,
    )?)?;
    Ok(())
}
