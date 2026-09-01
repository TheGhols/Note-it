use noteit_cli::output::OutputContext;
use noteit_cli::run_with_args;
use std::process::ExitCode;

fn main() -> ExitCode {
    let ctx = OutputContext::for_stdout();
    let args = std::env::args_os();

    match run_with_args(args, &ctx) {
        Ok(stdout) => {
            print!("{stdout}");
            ExitCode::from(noteit_cli::EXIT_SUCCESS)
        }
        Err((exit_code, stderr)) => {
            eprint!("{stderr}");
            ExitCode::from(exit_code)
        }
    }
}
