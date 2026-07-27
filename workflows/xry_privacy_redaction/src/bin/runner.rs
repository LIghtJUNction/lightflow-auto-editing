fn main() -> std::process::ExitCode {
    run().map_or_else(
        |error| {
            eprintln!("XRY privacy-redaction runner: {error}");
            std::process::ExitCode::FAILURE
        },
        |()| std::process::ExitCode::SUCCESS,
    )
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let request = lightflow::runner::read_request_from_stdin()?;
    request.validate_for(
        lightflow_xry_privacy_redaction::WORKFLOW_ID,
        lightflow_xry_privacy_redaction::WORKFLOW_VERSION,
    )?;
    lightflow::runner::write_response_to_stdout(&lightflow_xry_privacy_redaction::execute(
        &request.inputs,
    )?)?;
    Ok(())
}
