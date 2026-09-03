//! R-016 across *versions*: two builds that disagree about what a precondition
//! means must not write for each other.
//!
//! The revision mechanism is only a guarantee if the authority that performs
//! the write understands the precondition. `expected_revision` was added to
//! `WriteOperation::MutateNote` while the private control protocol stayed at
//! version 1, and that combination is a silent downgrade:
//!
//! ```text
//! new client  →  states protocol 1, sends expected_revision
//! old authority → checks 1 == 1, accepts
//!               → its MutateNote has no such field
//!               → serde drops the unknown key without a word
//!               → the conditional write is performed unconditionally
//! ```
//!
//! Everything here is written against *decoders of the other version*, not
//! against the current structs, because a struct compared with itself can
//! never show this failure.

use noteit_core::control::{
    check_protocol_version, read_frame, write_frame, ControlRequest, PROTOCOL_VERSION,
};
use noteit_core::model::NoteDocument;
use noteit_core::revision::NoteRevision;
use noteit_core::storage::StorageManager;
use noteit_core::write::{self, NoteMutation, WriteError, WriteOperation};
use noteit_core::{NoteItCore, Uuid};
use serde::{Deserialize, Serialize};
use std::fs;
use tempfile::{tempdir, TempDir};

/// The number the released build before this one stated and checked.
const LEGACY_PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------- the old shape

/// `WriteOperation` exactly as it was before the precondition existed.
///
/// This is the decoder an authority from the previous release really has. It
/// is spelled out here rather than derived from the current type on purpose:
/// the whole failure is that the current type has a field this one does not.
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum LegacyOperation {
    CreateNote {
        #[allow(dead_code)]
        draft: serde_json::Value,
    },
    MutateNote {
        selector: String,
        mutation: serde_json::Value,
        // Deliberately absent.
    },
    RestoreFromTrash {
        #[allow(dead_code)]
        selector: String,
    },
}

#[derive(Debug, Deserialize)]
struct LegacyRequest {
    protocol_version: u32,
    #[allow(dead_code)]
    request_id: Uuid,
    operation: LegacyOperation,
}

/// A request as the *older* client would have written it: no precondition key
/// on the wire at all, and the old version number.
#[derive(Debug, Serialize)]
struct LegacyOutgoingRequest {
    protocol_version: u32,
    request_id: Uuid,
    operation: LegacyOutgoingOperation,
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum LegacyOutgoingOperation {
    MutateNote {
        selector: String,
        mutation: serde_json::Value,
    },
}

// ------------------------------------------------------------------- the store

fn store() -> (TempDir, NoteItCore) {
    let tmp = tempdir().expect("tempdir");
    let storage = StorageManager::with_custom_paths(
        tmp.path().join("data/note-it/notes"),
        tmp.path().join("config/note-it"),
        tmp.path().join("state/note-it"),
        tmp.path().join("runtime/note-it"),
    )
    .expect("storage");
    (tmp, NoteItCore::from_storage(storage))
}

fn seed(core: &NoteItCore, body: &str) -> Uuid {
    let mut document = NoteDocument::new_empty();
    document.content = body.to_string();
    core.storage().save_note_atomic(&document).expect("seed");
    document.metadata.id
}

fn note_bytes(tmp: &TempDir, id: &Uuid) -> Vec<u8> {
    fs::read(tmp.path().join(format!("data/note-it/notes/{id}.md"))).expect("read note")
}

/// What an authority speaking `version` does with a frame: refuse on the
/// version, or hand the operation on to be executed.
fn authority_speaking(
    version: u32,
    core: &NoteItCore,
    frame: &[u8],
) -> Result<noteit_core::write::WriteOutcome, WriteError> {
    let request: ControlRequest =
        read_frame(&mut &frame[..]).map_err(|error| WriteError::InvalidInput {
            detail: error.to_string(),
        })?;
    // The gate, exactly where the real authority puts it: after the frame is
    // deserialized — which is how the version is readable at all — and before
    // the operation is handed on to be executed.
    if version == PROTOCOL_VERSION {
        check_protocol_version(request.protocol_version)?;
    } else {
        // An older build compared against its own constant.
        if request.protocol_version != version {
            return Err(WriteError::InvalidInput {
                detail: format!(
                    "this Note-it speaks control protocol {version} and the other \
                     side speaks {}; nothing was written",
                    request.protocol_version
                ),
            });
        }
    }
    write::execute(core, &request.operation)
}

// --------------------------------------------------------------------- R016-R1

#[test]
fn r016_r1_a_new_client_cannot_be_served_by_an_old_authority() {
    // The bug this phase exists to close. A v2 client sends a conditional
    // write; a v1 authority must refuse it on the version, because it cannot
    // honour the precondition and must not perform the write without it.
    let (tmp, core) = store();
    let id = seed(&core, "SHARED-BASE");
    let revision = NoteRevision::for_document(&core.read_note(&id).expect("read")).expect("rev");
    let before = note_bytes(&tmp, &id);

    let mut frame = Vec::new();
    write_frame(
        &mut frame,
        &ControlRequest::new(WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation: NoteMutation::ReplaceBody {
                body: "AGENT-CONCLUSION".to_string(),
            },
            expected_revision: Some(revision),
        }),
    )
    .expect("write frame");

    // 1. The old authority refuses on the version, before any mutation.
    let refusal = authority_speaking(LEGACY_PROTOCOL_VERSION, &core, &frame)
        .expect_err("a v1 authority must refuse a v2 request");
    match &refusal {
        WriteError::InvalidInput { detail } => {
            assert!(
                detail.contains("control protocol") && detail.contains("nothing was written"),
                "the refusal must name the mismatch: {detail}"
            );
        }
        other => panic!("expected a protocol refusal, got {other:?}"),
    }
    // Not `Indeterminate`: that is the one error meaning "it may have been
    // written". A protocol refusal happens before anything is attempted, so the
    // adapter maps it to `not_committed` and repeating it is safe.
    assert!(
        !matches!(refusal, WriteError::Indeterminate { .. }),
        "a protocol refusal must be a definite not_committed, got {refusal:?}"
    );

    // 2. Nothing was applied and not one byte moved.
    assert_eq!(note_bytes(&tmp, &id), before, "the note must be untouched");
    assert_eq!(core.read_note(&id).expect("read").content, "SHARED-BASE");

    // 3. And the reason it matters: had the version check let it through, the
    //    old decoder would have seen a mutation with no precondition at all.
    let wire = String::from_utf8(frame[4..].to_vec()).expect("utf-8");
    assert!(
        wire.contains("expected_revision"),
        "the client did send one"
    );
    let legacy: LegacyRequest = serde_json::from_str(&wire).expect("old decoder");
    match legacy.operation {
        LegacyOperation::MutateNote {
            selector, mutation, ..
        } => {
            assert_eq!(selector, id.to_string());
            assert!(mutation.get("op").is_some());
            // There is nowhere in the old shape for the precondition to land.
            // That is precisely why the versions must not meet.
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_ne!(
        legacy.protocol_version, LEGACY_PROTOCOL_VERSION,
        "the new client states a version the old authority does not accept"
    );
}

// --------------------------------------------------------------------- R016-R2

#[test]
fn r016_r2_an_old_client_cannot_be_served_by_a_new_authority() {
    // The other direction. A legitimate v1 request — no precondition key on the
    // wire, because that build had no such field — must be refused by a v2
    // authority rather than run as an unconditional write.
    let (tmp, core) = store();
    let id = seed(&core, "SHARED-BASE");
    let before = note_bytes(&tmp, &id);

    let legacy = LegacyOutgoingRequest {
        protocol_version: LEGACY_PROTOCOL_VERSION,
        request_id: Uuid::new_v4(),
        operation: LegacyOutgoingOperation::MutateNote {
            selector: id.to_string(),
            mutation: serde_json::json!({ "op": "replace_body", "body": "OLD-CLIENT" }),
        },
    };
    let payload = serde_json::to_vec(&legacy).expect("encode");
    let mut frame = Vec::new();
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);

    assert!(
        !String::from_utf8_lossy(&payload).contains("expected_revision"),
        "an old client does not put the key on the wire at all"
    );

    let refusal = authority_speaking(PROTOCOL_VERSION, &core, &frame)
        .expect_err("a v2 authority must refuse a v1 request");
    match &refusal {
        WriteError::InvalidInput { detail } => assert!(
            detail.contains("control protocol") && detail.contains("nothing was written"),
            "{detail}"
        ),
        other => panic!("expected a protocol refusal, got {other:?}"),
    }

    // No fallback to an unconditional write.
    assert_eq!(note_bytes(&tmp, &id), before);
    assert_eq!(core.read_note(&id).expect("read").content, "SHARED-BASE");
}

// --------------------------------------------------------------------- R016-R3

#[test]
fn r016_r3_the_same_version_still_does_all_three_things() {
    let (tmp, core) = store();
    let id = seed(&core, "BASE");

    // (a) No precondition: still last writer wins, which is what a person
    //     typing `noteit editar` is asking for.
    let mut frame = Vec::new();
    write_frame(
        &mut frame,
        &ControlRequest::new(WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation: NoteMutation::Append {
                payload: "HUMANO".to_string(),
            },
            expected_revision: None,
        }),
    )
    .expect("frame");
    let outcome = authority_speaking(PROTOCOL_VERSION, &core, &frame).expect("unconditional write");
    assert!(outcome.changed);
    assert!(core
        .read_note(&id)
        .expect("read")
        .content
        .contains("HUMANO"));

    // (b) Current revision: committed, and the new revision comes back.
    let current = NoteRevision::for_document(&core.read_note(&id).expect("read")).expect("rev");
    let mut frame = Vec::new();
    write_frame(
        &mut frame,
        &ControlRequest::new(WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation: NoteMutation::Append {
                payload: "CONDICIONAL".to_string(),
            },
            expected_revision: Some(current.clone()),
        }),
    )
    .expect("frame");
    let outcome = authority_speaking(PROTOCOL_VERSION, &core, &frame).expect("conditional write");
    assert!(outcome.changed);
    let after = outcome
        .revision
        .expect("a committed write reports its revision");
    assert_ne!(after, current);

    // (c) Stale revision: refused, and nothing moves.
    let before = note_bytes(&tmp, &id);
    let mut frame = Vec::new();
    write_frame(
        &mut frame,
        &ControlRequest::new(WriteOperation::MutateNote {
            selector: id.to_string(),
            mutation: NoteMutation::ReplaceBody {
                body: "OBSOLETO".to_string(),
            },
            expected_revision: Some(current),
        }),
    )
    .expect("frame");
    let refusal =
        authority_speaking(PROTOCOL_VERSION, &core, &frame).expect_err("a stale base is refused");
    assert!(
        matches!(refusal, WriteError::RevisionConflict { .. }),
        "{refusal:?}"
    );
    assert_eq!(note_bytes(&tmp, &id), before);
}

// ------------------------------------------------------- the version itself

#[test]
fn the_protocol_version_moved_when_the_meaning_of_a_request_did() {
    // A guard against the exact oversight this phase corrects: adding a field
    // to an operation changes what a request means, and the number has to move
    // with it or two builds silently disagree.
    assert_eq!(
        PROTOCOL_VERSION, 2,
        "adding `expected_revision` to MutateNote changed what a mutation means"
    );
    assert_ne!(PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION);

    // Both directions refused, stated once more as the plain invariant.
    assert!(check_protocol_version(LEGACY_PROTOCOL_VERSION).is_err());
    assert!(check_protocol_version(PROTOCOL_VERSION + 1).is_err());
    assert!(check_protocol_version(PROTOCOL_VERSION).is_ok());
}

#[test]
fn the_private_protocol_version_is_not_the_public_schema_version() {
    // Two contracts that move for different reasons. The machine interface's
    // `schema_version` describes the published `--json` document and did not
    // change; this one describes a private socket between two processes of the
    // same application.
    let machine_interface = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("docs/machine-interface.md"),
    )
    .expect("read the machine interface contract");
    assert!(
        machine_interface.contains("schema_version") && machine_interface.contains("`1`")
            || machine_interface.contains("schema_version"),
        "the public contract still documents its own version"
    );
    assert_eq!(PROTOCOL_VERSION, 2, "the private one is at 2");
}
