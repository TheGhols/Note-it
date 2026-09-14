//! A displayed document and its revision are one immutable read snapshot.
use noteit_core::{model::NoteDocument, revision::NoteRevision, write, Uuid};
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct LoadedDocument {
    pub id: Uuid,
    pub document: NoteDocument,
    pub revision: NoteRevision,
    pub in_trash: bool,
    /// The note's human label, derived once when it is read.
    ///
    /// `search::label_for` walks the body to find it, and the titles of three
    /// panes asked for it on every single frame — 2.2 ms of a 3.0 ms frame on
    /// a 100 KB note, growing with the note while the viewport did not. The
    /// label is a function of the content of *this snapshot*, and a snapshot
    /// is immutable, so asking more than once could only ever get the same
    /// answer more slowly.
    pub label: String,
}

impl LoadedDocument {
    pub fn new(document: NoteDocument, in_trash: bool) -> Result<Self, write::WriteError> {
        Ok(Self {
            id: document.metadata.id,
            revision: write::revision_of(&document)?,
            label: noteit_core::search::label_for(&document.content),
            in_trash,
            document,
        })
    }
}

impl Deref for LoadedDocument {
    type Target = NoteDocument;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}
