//! The desktop instance as the store's writer.
//!
//! While Note-it is running it owns the store, and it owns it from before it
//! can save anything until after it last can. That is not a policy choice made
//! here; it follows from what the application is. A note open in a window may
//! hold text that is not on disk yet, so the only process that can safely
//! write that note is the one that can ask the window for it. Everyone else
//! has to ask this one.
//!
//! So this module does two things:
//!
//! 1. takes the writer lease at startup and holds it for the whole session;
//! 2. listens on a private local socket, and carries out the changes other
//!    Note-it processes ask for.
//!
//! ## Why a thread and a channel
//!
//! Reading a socket blocks, and the GTK main loop must not. So the socket is
//! read on ordinary threads and every request is handed to the main loop,
//! which is where the store, the windows and the editor live. Nothing about a
//! note is touched off the main thread: the worker threads only move bytes.
//!
//! The main loop handles one request at a time, start to finish, and that is
//! deliberate. Two external writes overlapping would each take their own
//! snapshot of the same note and the second commit would silently undo the
//! first. Serialising them costs a few milliseconds of waiting and removes the
//! whole class of failure.
//!
//! ## Why requests are remembered
//!
//! If a connection breaks after the change was committed, the client cannot
//! tell whether it happened. It is told so and does not retry — but a client
//! that does retry with the same request identifier gets the recorded answer
//! rather than a second append. The window of memory is small and in-process;
//! it is a safety net under the rule, not the rule.

use crate::app::NoteItAppClone;
use crate::note_window::NoteWindow;
use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_channel::oneshot;
use futures_util::StreamExt;
use noteit_core::control::{
    check_protocol_version, read_frame, write_frame, ControlRequest, ControlResponse, ControlResult,
};
use noteit_core::coordination::{narrow_socket_file, WriteCoordinationPaths, WriterLease};
use noteit_core::diagnostics;
use noteit_core::model::NoteDocument;
use noteit_core::revision::NoteRevision;
use noteit_core::storage::StorePaths;
use noteit_core::write::{self, NoteMutation, WriteError, WriteOperation, WriteOutcome};
use std::collections::VecDeque;
use std::fmt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// How long a worker thread waits for the main loop to answer one request.
///
/// Comfortably longer than the editor barrier it may have to wait for, so this
/// is a backstop against a main loop that has stopped answering rather than a
/// second deadline competing with the first.
const REPLY_TIMEOUT: Duration = noteit_core::coordination::PROTOCOL_DESKTOP_WORKER_TIMEOUT;

/// How long a client is given to send its request once connected.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// How many answered requests are remembered, so a repeat is recognised.
const REMEMBERED_REQUESTS: usize = 64;

/// How long startup waits for a store somebody else is holding.
///
/// Sized for the one case that is genuinely transient: a `noteit` command
/// finishing its own direct write, which takes single-digit milliseconds.
/// Beyond that the store is really held, and starting anyway would make this
/// process a second writer.
const LEASE_WAIT: Duration = Duration::from_millis(1500);

/// Why a desktop instance refused to start.
///
/// Every variant means the same thing about the store: this process is not its
/// writer, and it must not behave as though it were.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupRefusal {
    /// The runtime coordination directory could not be created or trusted.
    Coordination(String),
    /// Another Note-it writer holds this store and did not let go.
    StoreHeld,
    /// The lease could not even be tested for.
    LeaseUnavailable(String),
    /// The control socket could not be opened or made private.
    Socket(String),
}

impl fmt::Display for StartupRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StoreHeld => formatter.write_str(
                "O Note-it não pôde iniciar: o repositório está sendo usado por outro \
                 escritor do Note-it. Feche a outra instância e tente de novo. \
                 Nada foi alterado.",
            ),
            Self::Coordination(detail) => write!(
                formatter,
                "O Note-it não pôde iniciar: o diretório de coordenação de escrita não \
                 é utilizável, então esta instância não pode garantir que é a única a \
                 gravar. Nada foi alterado. Detalhe: {detail}"
            ),
            Self::LeaseUnavailable(detail) => write!(
                formatter,
                "O Note-it não pôde iniciar: não foi possível verificar quem está \
                 gravando no repositório. Nada foi alterado. Detalhe: {detail}"
            ),
            Self::Socket(detail) => write!(
                formatter,
                "O Note-it não pôde iniciar: o canal de controle não pôde ser aberto, \
                 então a linha de comando não teria como falar com esta instância. \
                 Nada foi alterado. Detalhe: {detail}"
            ),
        }
    }
}

impl std::error::Error for StartupRefusal {}

/// The lease and the socket, held for the life of the application.
///
/// **A desktop instance cannot exist without one of these.** `AppContext` holds
/// it by value rather than as an `Option`, and the only way to build one is
/// [`claim`], which returns it solely on complete success — lease taken, socket
/// bound, socket narrowed. So "a Note-it window that can save while something
/// else owns the store" is not a state this program can get into by mistake;
/// it is a state the types do not allow it to describe.
///
/// Dropping this releases the store. Nothing in the application drops it early:
/// the lease is released when the process ends, which is the only moment at
/// which the desktop instance stops being able to save.
pub struct WriteAuthority {
    _lease: WriterLease,
    socket_path: PathBuf,
}

impl Drop for WriteAuthority {
    fn drop(&mut self) {
        // Tidiness rather than correctness: a socket left behind is refused a
        // connection by the kernel, and the next holder of the lease unlinks
        // it before binding its own.
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// One request on its way to the main loop, and the way back.
struct Envelope {
    request: ControlRequest,
    reply: std::sync::mpsc::Sender<ControlResponse>,
}

/// The store, claimed — and the requests that have not been served yet.
///
/// Claiming and serving are two steps because the server needs a controller and
/// the controller needs an `AppContext`, which in turn needs the claim. Rather
/// than break that circle with an `Option` that could be left empty, it is
/// broken in the middle: the claim happens first and produces the authority, the
/// application is built around it, and only then is the controller handed to
/// [`serve`].
pub struct StoreClaim {
    authority: WriteAuthority,
    requests: futures_channel::mpsc::UnboundedReceiver<Envelope>,
}

impl StoreClaim {
    /// Splits the claim into the value the application holds and the stream the
    /// server consumes.
    pub fn split(self) -> (WriteAuthority, PendingRequests) {
        (self.authority, PendingRequests(self.requests))
    }
}

/// Requests waiting to be served, once there is an application to serve them.
pub struct PendingRequests(futures_channel::mpsc::UnboundedReceiver<Envelope>);

/// Takes the store, or refuses to start.
///
/// Every step here is a condition of being the writer, and there is no partial
/// success: a lease without a socket is not an authority, because the command
/// line would have no way to reach the process that holds the store. When any
/// step fails the lease is dropped on the way out — released before this
/// returns — so the next writer is not shut out by a process that gave up.
///
/// The short wait exists for one specific case: a `noteit` command finishing a
/// direct write, which takes milliseconds. Anything longer than that is a store
/// somebody else genuinely holds, and the honest answer is to say so rather than
/// to open a window that would quietly become a second writer.
pub fn claim(paths: &StorePaths) -> Result<StoreClaim, StartupRefusal> {
    let coordination = WriteCoordinationPaths::for_store(paths)
        .map_err(|error| StartupRefusal::Coordination(error.to_string()))?;
    coordination
        .prepare()
        .map_err(|error| StartupRefusal::Coordination(error.to_string()))?;

    let lease = match WriterLease::acquire_within(
        &coordination,
        std::time::Instant::now() + LEASE_WAIT,
        Duration::from_millis(25),
    ) {
        Ok(Some(lease)) => lease,
        Ok(None) => return Err(StartupRefusal::StoreHeld),
        Err(error) => return Err(StartupRefusal::LeaseUnavailable(error.to_string())),
    };

    let socket_path = coordination.socket_path();
    // Safe precisely because the lease is held: no live authority can own this
    // socket while this process holds the store, so anything at that path was
    // left by a process that is gone.
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .map_err(|error| StartupRefusal::Socket(error.to_string()))?;
    if let Err(error) = narrow_socket_file(&socket_path) {
        // Removed rather than left lying around: a socket nobody can trust the
        // permissions of is worse than no socket at all.
        let _ = std::fs::remove_file(&socket_path);
        return Err(StartupRefusal::Socket(error.to_string()));
    }

    let (sender, receiver) = unbounded::<Envelope>();

    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(stream) = connection else { continue };
            let sender = sender.clone();
            // One thread per connection, so a client that connects and says
            // nothing cannot keep the next one out.
            std::thread::spawn(move || serve_connection(stream, sender));
        }
    });

    diagnostics::log(format_args!(
        "event=write-authority-claimed store={}",
        coordination.store_key()
    ));

    Ok(StoreClaim {
        authority: WriteAuthority {
            _lease: lease,
            socket_path,
        },
        requests: receiver,
    })
}

/// Starts answering the requests this process has already claimed the right to.
pub fn serve(controller: NoteItAppClone, pending: PendingRequests) {
    let mut receiver = pending.0;
    let mut remembered: VecDeque<(Uuid, ControlResult)> = VecDeque::new();
    glib::MainContext::default().spawn_local(async move {
        // One at a time, on purpose: see the module comment.
        while let Some(envelope) = receiver.next().await {
            let request_id = envelope.request.request_id;
            let result = match recall(&remembered, request_id) {
                Some(previous) => previous,
                None => {
                    let result = handle(&controller, &envelope.request).await;
                    remember(&mut remembered, request_id, result.clone());
                    result
                }
            };
            let _ = envelope.reply.send(ControlResponse {
                protocol_version: noteit_core::control::PROTOCOL_VERSION,
                request_id,
                result,
            });
        }
    });
}

/// Reads one request, waits for the answer, writes it back.
///
/// Deliberately one request per connection. The client opens a socket, asks
/// one thing and reads one answer; there is no session, nothing to keep in
/// step, and nothing left half-said if either side goes away.
fn serve_connection(mut stream: UnixStream, sender: UnboundedSender<Envelope>) {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let _ = stream.set_write_timeout(Some(REQUEST_TIMEOUT));

    let request: ControlRequest = match read_frame(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            // A frame that is malformed, oversized or cut short is answered
            // with nothing at all and the connection is dropped. It never
            // reaches the main loop, so it can never reach a note.
            eprintln!("Rejected a control frame: {error}");
            return;
        }
    };

    if let Err(error) = check_protocol_version(request.protocol_version) {
        let _ = write_frame(
            &mut stream,
            &ControlResponse::refused(request.request_id, error),
        );
        return;
    }

    let (reply, answer) = std::sync::mpsc::channel();
    if sender.unbounded_send(Envelope { request, reply }).is_err() {
        return;
    }

    match answer.recv_timeout(REPLY_TIMEOUT) {
        Ok(response) => {
            let _ = write_frame(&mut stream, &response);
        }
        Err(_) => {
            eprintln!("A control request was not answered in time and the connection was closed.");
        }
    }
}

fn recall(remembered: &VecDeque<(Uuid, ControlResult)>, request_id: Uuid) -> Option<ControlResult> {
    remembered
        .iter()
        .find(|(id, _)| *id == request_id)
        .map(|(_, result)| result.clone())
}

fn remember(
    remembered: &mut VecDeque<(Uuid, ControlResult)>,
    request_id: Uuid,
    result: ControlResult,
) {
    remembered.push_back((request_id, result));
    while remembered.len() > REMEMBERED_REQUESTS {
        remembered.pop_front();
    }
}

async fn handle(controller: &NoteItAppClone, request: &ControlRequest) -> ControlResult {
    match apply_operation(controller, &request.operation).await {
        Ok(outcome) => ControlResult::Committed(Box::new(outcome)),
        Err(error) => ControlResult::Refused(Box::new(error)),
    }
}

/// Carries out one operation as the store's authority.
///
/// Creating a note and restoring one from the trash never involve a window:
/// they are pure store operations, and doing them here must not open anything,
/// focus anything or record a note as open. `noteit criar` behaves the same
/// whether Note-it is running or not, and that is the point of it.
pub async fn apply_operation(
    controller: &NoteItAppClone,
    operation: &WriteOperation,
) -> Result<WriteOutcome, WriteError> {
    match operation {
        WriteOperation::CreateNote { .. } | WriteOperation::RestoreFromTrash { .. } => {
            let core = controller.context.borrow().core.clone();
            write::execute(&core, operation)
        }
        WriteOperation::MutateNote {
            selector,
            mutation,
            expected_revision,
        } => {
            let core = controller.context.borrow().core.clone();
            let note_id = core.resolve_note_id(selector)?;
            let window = controller.context.borrow().windows.get(&note_id).cloned();

            match window {
                // No window: nothing holds unsaved text, so the file is the
                // whole truth and this is an ordinary Core write.
                None => write::execute(&core, operation),
                Some(window) if !window.is_loaded() => {
                    // A window whose page has not loaded yet holds no text of
                    // its own. The document in memory is the file, so it is
                    // mutated directly and the page will be handed the
                    // committed version when it asks for one.
                    mutate_unloaded(controller, &window, mutation, expected_revision)
                }
                Some(window) => {
                    mutate_open_note(controller, &window, mutation, expected_revision).await
                }
            }
        }
    }
}

fn mutate_unloaded(
    controller: &NoteItAppClone,
    window: &NoteWindow,
    mutation: &NoteMutation,
    expected_revision: &Option<NoteRevision>,
) -> Result<WriteOutcome, WriteError> {
    let base = window.document.borrow().clone();
    let note_id = base.metadata.id;
    let kind = mutation.outcome_kind();

    // The document in memory *is* the file for a page that has not loaded, so
    // this is the base the mutation will be applied to and the one the
    // precondition is about.
    let base_revision = write::ensure_revision_matches(&note_id, &base, expected_revision)?;

    let Some(candidate) = write::apply(&base, mutation)? else {
        return Ok(WriteOutcome::new(note_id, kind, false).with_revision(base_revision));
    };
    let core = controller.context.borrow().core.clone();
    write::commit_addressed(&core, &note_id, &candidate)?;
    let committed_revision = write::revision_of(&candidate)?;
    window.adopt_committed_document(candidate);
    Ok(WriteOutcome::new(note_id, kind, true).with_revision(committed_revision))
}

/// The whole pipeline for a note somebody has open.
///
/// Every step is load-bearing and none of them may be skipped:
///
/// 1. refuse if the window is busy being hidden, quit or deleted;
/// 2. freeze the editor, and only then take its text;
/// 3. fold that text into the document, so nothing unsaved is lost;
/// 4. apply the mutation to *that*;
/// 5. commit through the canonical atomic writer;
/// 6. adopt the committed note in the host and move the generation on, so
///    anything still in flight from the previous run is refused;
/// 7. hand the committed note back to the page and let it edit again.
async fn mutate_open_note(
    controller: &NoteItAppClone,
    window: &NoteWindow,
    mutation: &NoteMutation,
    expected_revision: &Option<NoteRevision>,
) -> Result<WriteOutcome, WriteError> {
    let note_id = window.id;
    let kind = mutation.outcome_kind();
    let request_id = Uuid::new_v4();

    controller
        .begin_external_write()
        .map_err(|detail| WriteError::WriterBusy { detail })?;

    let snapshot = {
        let (sender, receiver) = oneshot::channel();
        window.begin_external_write(request_id, move |result| {
            let _ = sender.send(result);
        });
        receiver
            .await
            .unwrap_or_else(|_| Err("a nota aberta foi fechada durante a alteração".to_string()))
    };

    let markdown = match snapshot {
        Ok(markdown) => markdown,
        Err(detail) => {
            controller.finish_external_write();
            // Nothing was written and the editor is free again, so this is a
            // plain refusal and repeating the command is safe.
            return Err(WriteError::WriterBusy { detail });
        }
    };

    // The document as it really is: what the host committed last, with the
    // editor's unsaved text folded in, and the mutation applied to *that*.
    // Mutating the file's version instead is exactly how an unsaved paragraph
    // disappears. The rule lives in the Core so the direct path and this one
    // cannot drift, and so it can be proven without a compositor.
    let base = window.document.borrow().clone();
    // The precondition is checked inside, against that same folded base and
    // before any mutation touches it — so a client whose revision predates the
    // paragraph now sitting unsaved in the editor is refused rather than
    // allowed to write over it.
    let live = match write::apply_over_live_body(&base, &markdown, mutation, expected_revision) {
        Ok(live) => live,
        Err(error) => {
            window.abort_external_write(request_id);
            controller.finish_external_write();
            return Err(error);
        }
    };
    let mutation_changed = live.mutation_changed;
    let base_revision = live.base_revision.clone();

    let Some(candidate) = live.candidate else {
        window.abort_external_write(request_id);
        controller.finish_external_write();
        return Ok(WriteOutcome::new(note_id, kind, false).with_revision(base_revision));
    };

    let core = controller.context.borrow().core.clone();
    if let Err(error) = write::commit_addressed(&core, &note_id, &candidate) {
        // Before the commit point. The file is untouched, the host still
        // describes it, and the page keeps the text it had.
        window.abort_external_write(request_id);
        controller.finish_external_write();
        return Err(error);
    }

    // Past the commit point. From here nothing may be reported as a failure.
    *window.document.borrow_mut() = candidate.clone();
    let synced = confirm_ui_sync(window, request_id, &candidate).await;
    controller.finish_external_write();
    // The reason, never the note. These are fixed sentences chosen by the host,
    // so nothing a person wrote can reach a log through here.
    diagnostics::log(format_args!(
        "event=external-write-committed note={note_id} generation={} ui_synced={} reason={}",
        window.external_generation(),
        synced.is_ok(),
        synced.as_ref().err().map_or("-", String::as_str)
    ));

    Ok(committed_outcome(
        WriteOutcome::new(note_id, kind, mutation_changed)
            .with_revision(write::revision_of(&candidate)?),
        synced,
    ))
}

/// The answer for a write that has already reached the disk.
///
/// It returns a `WriteOutcome` and not a `Result`, and that is the whole point:
/// past the commit point there is no failure left to report. Whether the window
/// took the change on, refused it, or never said, the file is the new one — so
/// the only thing left to decide is whether the answer carries a warning.
///
/// Turning any of this into an error would be a lie with consequences: a caller
/// told an append failed repeats it, and the paragraph lands twice.
fn committed_outcome(outcome: WriteOutcome, synced: Result<(), String>) -> WriteOutcome {
    match synced {
        Ok(()) => outcome,
        Err(detail) => outcome.with_ui_sync_warning(detail),
    }
}

/// Hands the committed note to the page and waits to hear that it arrived.
///
/// The difference this draws is the one that matters most in the whole
/// pipeline. "The write did not happen" and "the write happened and the window
/// may not be showing it" look the same to a caller and are opposites: told
/// the first, a person repeats the command, and repeating an append that
/// already committed puts the paragraph in twice.
async fn confirm_ui_sync(
    window: &NoteWindow,
    request_id: Uuid,
    committed: &NoteDocument,
) -> Result<(), String> {
    let (sender, receiver) = oneshot::channel();
    window.finish_external_write(request_id, committed, move |result| {
        let _ = sender.send(result);
    });
    receiver.await.unwrap_or_else(|_| {
        Err("a janela desapareceu antes de confirmar a atualização".to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::committed_outcome;
    use noteit_core::write::{WriteOutcome, WriteOutcomeKind};
    use uuid::Uuid;

    fn appended() -> WriteOutcome {
        WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::ContentAppended, true)
    }

    #[test]
    fn a_window_that_confirmed_produces_no_warning() {
        let outcome = committed_outcome(appended(), Ok(()));
        assert!(outcome.ui_sync_warning.is_none());
        assert!(outcome.changed);
    }

    #[test]
    fn every_way_the_window_can_fail_is_still_a_committed_write() {
        // 4.0E.2 §32. Refused, timed out, undeliverable: the file changed in all
        // three, so all three answer with a warning on a successful write. The
        // signature says so — there is no error to return here — and these
        // cover the reasons that actually reach it.
        for reason in [
            "a nota aberta não conseguiu adotar o documento gravado",
            "a nota aberta não confirmou a atualização a tempo",
            "a mensagem não pôde ser entregue à página",
        ] {
            let outcome = committed_outcome(appended(), Err(reason.to_string()));
            assert_eq!(outcome.ui_sync_warning.as_deref(), Some(reason));
            assert!(
                outcome.changed,
                "a committed write stopped reporting that it changed something"
            );
        }
    }

    #[test]
    fn a_no_op_that_could_not_be_shown_is_still_a_no_op() {
        let unchanged = WriteOutcome::new(Uuid::new_v4(), WriteOutcomeKind::TagAdded, false);
        let outcome = committed_outcome(unchanged, Err("sem confirmação".to_string()));
        assert!(!outcome.changed);
        assert!(outcome.ui_sync_warning.is_some());
    }
}
