fn main() -> std::process::ExitCode {
    run().map_or_else(
        |error| {
            eprintln!("Video Work subtitles runner: {error}");
            std::process::ExitCode::FAILURE
        },
        |()| std::process::ExitCode::SUCCESS,
    )
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request = lightflow::runner::read_request_from_stdin()?;
    request.validate_for(
        lightflow_video_work_subtitles::WORKFLOW_ID,
        lightflow_video_work_subtitles::WORKFLOW_VERSION,
    )?;
    lightflow::runner::write_response_to_stdout(&lightflow_video_work_subtitles::execute(
        &request.inputs,
    )?)?;
    Ok(())
}
