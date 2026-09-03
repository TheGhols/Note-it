//! `noteit-mcp` — Note-it's Model Context Protocol server, over stdio.
//!
//! # Standard output belongs to the protocol
//!
//! Every byte on stdout is an MCP message. Not a banner, not a version line,
//! not a progress note, not a warning, not "starting…". A single stray
//! `println!` here corrupts the JSON-RPC stream and the host sees a parse
//! error instead of a server — so there is deliberately no printing in this
//! binary at all, and a test drives the real process to prove it.
//!
//! Diagnostics, when there is genuinely nothing else to do, go to stderr, and
//! even there they never carry a note's body, a clipboard, or a path that the
//! failure did not require.
//!
//! # No arguments, no configuration
//!
//! The host owns this process. There is no flag to change the store, no
//! configuration file read from the user's home, and nothing written into a
//! host's settings. Which store this speaks for is decided the same way every
//! other Note-it program decides it: the XDG environment it was started in.

use noteit_mcp::NoteItMcpServer;
use rmcp::transport::stdio;
use rmcp::ServiceExt;

fn main() -> std::process::ExitCode {
    // A current-thread runtime, which is enough because this thread does not
    // do the work: it reads standard input, routes, and writes answers, while
    // every Core call goes to Tokio's blocking pool through
    // `domain::off_reactor`. One thread on the protocol, other threads on the
    // disk — so a search that walks ten thousand notes does not stop the
    // server from answering `ping` or accepting the next request.
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return fail(&format!("the runtime could not be started: {error}")),
    };

    runtime.block_on(async {
        let service = match NoteItMcpServer::new().serve(stdio()).await {
            Ok(service) => service,
            // The host closing the stream before initialising is an ordinary
            // way for this process to end, not something to shout about.
            Err(error) => return fail(&format!("the MCP transport could not start: {error}")),
        };

        match service.waiting().await {
            Ok(_) => std::process::ExitCode::SUCCESS,
            Err(error) => fail(&format!("the MCP session ended abnormally: {error}")),
        }
    })
}

/// The only thing this binary ever writes for a person, and it writes it on
/// stderr.
fn fail(detail: &str) -> std::process::ExitCode {
    eprintln!("noteit-mcp: {detail}");
    std::process::ExitCode::FAILURE
}
