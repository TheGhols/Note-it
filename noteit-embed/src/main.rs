//! The worker's entry point.
//!
//! Arguments are deliberately two and neither is a URL, a host or a key. A
//! credential on a command line is visible in `ps`, in `/proc/<pid>/cmdline`
//! and in a shell history, which is why §24 names `argv` explicitly and
//! why there is no flag here that could carry one.
//!
//! ```text
//! noteit-embed --socket <path> [--config-dir <path>]
//! ```
//!
//! Everything it writes goes to standard error and is for a person: a socket
//! path and a refusal, never a key, never a note and never a provider's
//! sentence.

use noteit_embed::server::{Limits, Worker};
use noteit_embed::socket::{self, SocketPath};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let mut socket_path: Option<PathBuf> = None;
    let mut config_dir: Option<PathBuf> = None;

    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--socket") => socket_path = arguments.next().map(PathBuf::from),
            Some("--config-dir") => config_dir = arguments.next().map(PathBuf::from),
            Some("--help") | Some("-h") => {
                eprintln!("Uso: noteit-embed --socket <caminho> [--config-dir <caminho>]");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("noteit-embed: argumento desconhecido");
                return ExitCode::from(2);
            }
        }
    }

    let Some(socket_path) = socket_path else {
        eprintln!("noteit-embed: --socket é obrigatório");
        return ExitCode::from(2);
    };
    let config_dir = match config_dir {
        Some(directory) => directory,
        None => match dirs_config() {
            Some(directory) => directory,
            None => {
                eprintln!("noteit-embed: não foi possível resolver o diretório de configuração");
                return ExitCode::from(2);
            }
        },
    };

    // The test redirection is compiled out of every shipped build. With the
    // feature off this whole block does not exist, so a released binary has no
    // code that reads a base URL from anywhere.
    #[cfg(feature = "test-endpoints")]
    if let Ok(base) = std::env::var("NOTEIT_EMBED_TEST_BASE_URL") {
        for provider in noteit_embed_protocol::ProviderId::ALL {
            noteit_embed::endpoint::test_override::set_for_process(provider, base.clone());
        }
    }

    let validated = match SocketPath::validate(&socket_path) {
        Ok(validated) => validated,
        Err(error) => {
            eprintln!("noteit-embed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let listener = match validated.bind() {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("noteit-embed: {error}");
            return ExitCode::FAILURE;
        }
    };

    let worker = Arc::new(Worker::new(config_dir, Limits::default()));
    // The one line the spawner waits for. It carries a path this process was
    // given, and nothing it discovered.
    eprintln!("noteit-embed: pronto em {}", socket_path.display());
    worker.serve(listener);
    validated.remove();
    ExitCode::SUCCESS
}

/// `$XDG_CONFIG_HOME/note-it`, by the same rules the rest of Note-it uses.
///
/// Resolved here rather than taken from `noteit-core`, because depending on
/// the Core would put this process's HTTP stack one `cargo tree` edge away
/// from the crate that holds the store.
fn dirs_config() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("note-it"))
}

/// Kept so the module is not dead when the socket helpers move.
#[allow(dead_code)]
fn runtime_directory() -> std::io::Result<PathBuf> {
    socket::runtime_directory()
}
