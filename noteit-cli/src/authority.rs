//! Deciding who writes, and then writing.
//!
//! There is exactly one rule and this module is all of it:
//!
//! ```text
//! the writer lease is free   → this process writes, through the Core
//! the writer lease is held   → whoever holds it writes, and is asked over IPC
//! held, and unreachable      → nothing is written, and that is said plainly
//! ```
//!
//! The third line is the one that matters. A store held by another writer that
//! cannot be reached is not an invitation to write anyway "just this once": the
//! whole point of the lease is that two processes editing the same note lose
//! one of the edits, and a fallback to a direct write would reintroduce
//! precisely the failure the lease exists to prevent. So it fails closed, says
//! so, and changes nothing.
//!
//! ## Why there is a waiting window
//!
//! A held lease is usually held for milliseconds — another `noteit` command
//! finishing its own write — or by a desktop instance that has started but has
//! not bound its socket yet. Failing instantly on either would make two
//! simultaneous commands lose one for no reason. So a busy store is retried
//! for a short, bounded window, trying both the lease and the socket each
//! time, and then answered rather than waited on forever.
//!
//! ## Why an append is never retried by itself
//!
//! If the connection drops after the request went out, the authority may have
//! committed it already. Sending it again would append the same paragraph
//! twice, and there is no way from here to tell the two cases apart. So that
//! outcome is reported as unknown and the person is asked to look — which is
//! recoverable, where a duplicated note is a mess someone has to clean up by
//! hand.

use noteit_core::control::{
    check_protocol_version, read_frame, write_frame, ControlRequest, ControlResponse, ControlResult,
};
use noteit_core::coordination::{CoordinationError, WriteCoordinationPaths, WriterLease};
use noteit_core::storage::StorePaths;
use noteit_core::write::{self, WriteError, WriteOperation, WriteOutcome};
use noteit_core::NoteItCore;
use std::io;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// How long a busy store is retried before the command answers.
///
/// Long enough to cover another command finishing and a desktop instance
/// finishing its startup; short enough that "the store is busy" is an answer
/// rather than a hang.
const BUSY_RETRY_WINDOW: Duration = Duration::from_secs(3);
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// How long the authority is given to answer one request.
///
/// It covers freezing the editor, taking its text, writing the file and
/// putting the committed note back on screen. Generous, because the
/// alternative to waiting is an unknown outcome, and an unknown outcome is the
/// expensive one.
const AUTHORITY_TIMEOUT: Duration = noteit_core::coordination::PROTOCOL_CLI_AUTHORITY_TIMEOUT;

/// A runtime directory that cannot be trusted is reported as an unreachable
/// authority: from the caller's side both mean the same thing — the store was
/// not written and nothing was changed.
fn from_coordination(error: CoordinationError) -> WriteError {
    match error {
        CoordinationError::Unavailable(detail) | CoordinationError::Unsafe(detail) => {
            WriteError::AuthorityUnavailable { detail }
        }
    }
}

/// Which of the two paths a command took.
///
/// Reported so the CLI's own tests can assert the decision itself rather than
/// inferring it from what happened to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePath {
    /// Nobody held the store, so this process took the lease and wrote.
    Direct,
    /// A Note-it instance held the store and made the change.
    Authority,
}

/// A completed write and the path it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerformedWrite {
    pub outcome: WriteOutcome,
    pub path: WritePath,
}

/// Runs one operation against the store, whoever turns out to own it.
pub fn perform(operation: &WriteOperation) -> Result<PerformedWrite, WriteError> {
    perform_at(&StorePaths::resolve(), operation)
}

/// The same, against an explicitly resolved store. Used by the tests.
pub fn perform_at(
    paths: &StorePaths,
    operation: &WriteOperation,
) -> Result<PerformedWrite, WriteError> {
    let coordination = WriteCoordinationPaths::for_store(paths).map_err(from_coordination)?;
    coordination.prepare().map_err(from_coordination)?;

    let deadline = Instant::now() + BUSY_RETRY_WINDOW;
    // Assigned on every path that reaches the deadline check below, so the
    // refusal always says which of the two ways the authority was out of reach.
    let mut last_reason;

    loop {
        // Taking the lease is what makes a direct write safe: while it is held
        // no other Note-it writer can be running, so reading the note and
        // writing it back cannot lose anyone's edit.
        match WriterLease::try_acquire_prepared(&coordination) {
            Ok(Some(lease)) => {
                let outcome = write_directly(paths, operation);
                // Released only here, once the atomic commit has returned and
                // the result is known. Releasing any earlier would open the
                // window the lease exists to close.
                drop(lease);
                return outcome.map(|outcome| PerformedWrite {
                    outcome,
                    path: WritePath::Direct,
                });
            }
            Ok(None) => {}
            Err(error) => return Err(from_coordination(error)),
        }

        // Held. The holder is the authority, and it is asked rather than
        // worked around.
        match UnixStream::connect(coordination.socket_path()) {
            Ok(stream) => {
                return ask_authority(stream, operation).map(|outcome| PerformedWrite {
                    outcome,
                    path: WritePath::Authority,
                })
            }
            Err(error) => {
                last_reason = describe_connect_failure(&error);
            }
        }

        if Instant::now() >= deadline {
            return Err(WriteError::AuthorityUnavailable {
                detail: last_reason,
            });
        }
        std::thread::sleep(BUSY_RETRY_INTERVAL);
    }
}

fn write_directly(
    paths: &StorePaths,
    operation: &WriteOperation,
) -> Result<WriteOutcome, WriteError> {
    let storage = noteit_core::StorageManager::from_paths(paths.clone())
        .map_err(|detail| WriteError::StoreUnavailable { detail })?;
    let core = NoteItCore::from_storage(storage);
    write::execute(&core, operation)
}

/// Sends one request and waits for its answer.
fn ask_authority(
    mut stream: UnixStream,
    operation: &WriteOperation,
) -> Result<WriteOutcome, WriteError> {
    stream
        .set_read_timeout(Some(AUTHORITY_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(AUTHORITY_TIMEOUT)))
        .map_err(|error| WriteError::AuthorityUnavailable {
            detail: format!("the authority connection could not be configured: {error}"),
        })?;

    let request = ControlRequest::new(operation.clone());
    let request_id = request.request_id;

    // Everything up to here has written nothing anywhere. A failure sending
    // the request is therefore unambiguous: the authority never saw it.
    if let Err(error) = write_frame(&mut stream, &request) {
        return Err(WriteError::AuthorityUnavailable {
            detail: format!("the request could not be sent, so nothing was changed: {error}"),
        });
    }

    // Past this point the authority may already have committed. Nothing below
    // may resend the request.
    let response: ControlResponse = match read_frame(&mut stream) {
        Ok(response) => response,
        Err(error) => return Err(indeterminate(&format!("{error}"))),
    };

    check_protocol_version(response.protocol_version)?;
    if response.request_id != request_id {
        return Err(indeterminate("the answer did not belong to this request"));
    }

    match response.result {
        ControlResult::Committed(outcome) => Ok(*outcome),
        ControlResult::Refused(error) => Err(*error),
    }
}

/// The one answer that is neither success nor failure.
fn indeterminate(detail: &str) -> WriteError {
    WriteError::Indeterminate {
        detail: detail.to_string(),
    }
}

fn describe_connect_failure(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => {
            "the store is held by another Note-it writer that has not opened its control socket"
                .to_string()
        }
        io::ErrorKind::ConnectionRefused => {
            "the store is held by another Note-it writer that is not listening".to_string()
        }
        _ => format!("the authority could not be reached: {error}"),
    }
}
