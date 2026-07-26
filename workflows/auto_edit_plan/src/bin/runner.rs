fn main() -> std::process::ExitCode {
    run().map_or_else(
        |error| {
            eprintln!("video auto-edit plan runner: {error}");
            std::process::ExitCode::FAILURE
        },
        |()| std::process::ExitCode::SUCCESS,
    )
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request = lightflow::runner::read_request_from_stdin()?;
    request.validate_for(
        lightflow_video_auto_edit_plan::WORKFLOW_ID,
        lightflow_video_auto_edit_plan::WORKFLOW_VERSION,
    )?;
    lightflow::runner::write_response_to_stdout(&lightflow_video_auto_edit_plan::execute(
        &request.inputs,
    )?)?;
    Ok(())
}
