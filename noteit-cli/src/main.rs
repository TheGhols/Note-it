use noteit_cli::output::Channels;
use noteit_cli::run_with_args;
use std::process::ExitCode;

fn main() -> ExitCode {
    let channels = Channels::detect();
    let response = run_with_args(std::env::args_os(), &channels);

    // Written in the order they have always appeared: whatever a command had
    // to warn about, then its result. In machine mode exactly one of the two
    // is ever non-empty.
    if !response.stderr.is_empty() {
        eprint!("{}", response.stderr);
    }
    if !response.stdout.is_empty() {
        print!("{}", response.stdout);
    }

    ExitCode::from(response.exit_code)
}
