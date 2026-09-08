//! A displayed document and its revision are one immutable read snapshot.
use noteit_core::{model::NoteDocument, revision::NoteRevision, write, Uuid};
use std::ops::Deref;

#[derive(Debug, Clone)]
pub struct LoadedDocument {
    pub id: Uuid,
    pub document: NoteDocument,
    pub revision: NoteRevision,
    pub in_trash: bool,
}

impl LoadedDocument {
    pub fn new(document: NoteDocument, in_trash: bool) -> Result<Self, write::WriteError> {
        Ok(Self {
            id: document.metadata.id,
            revision: write::revision_of(&document)?,
            document,
            in_trash,
        })
    }
}

impl Deref for LoadedDocument {
    type Target = NoteDocument;
    fn deref(&self) -> &Self::Target {
        &self.document
    }
}
