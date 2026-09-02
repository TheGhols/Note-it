use noteit_cli::output::OutputContext;
use noteit_cli::run_with_args;
use std::process::ExitCode;

fn main() -> ExitCode {
    let ctx = OutputContext::for_stdout();
    let response = run_with_args(std::env::args_os(), &ctx);

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
