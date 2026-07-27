use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = std::io::stdin().lock();
    let error = match lightflow_xry_gateway_handler::run_staging(&mut input) {
        Err(error) => error,
        Ok(never) => match never {},
    };
    eprintln!("lightflow xry gateway handler: {error}");
    ExitCode::FAILURE
}
