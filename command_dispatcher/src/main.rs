fn main() -> std::process::ExitCode {
    lightflow_auto_editing_command_dispatcher::run_from_stdio().map_or_else(
        |error| {
            eprintln!("lightflow auto-editing command dispatcher: {error}");
            std::process::ExitCode::FAILURE
        },
        |()| std::process::ExitCode::SUCCESS,
    )
}
