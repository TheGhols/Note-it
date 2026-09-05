//! The semantic half of retrieval, with no provider in it.
//!
//! 4.3B builds the frame and stops there, on purpose. No model is downloaded,
//! no inference crate is chosen, no network exists, and production still runs
//! with no provider configured at all — [`crate::context::retrieve`] cannot
//! reach any of this, because the mode it uses does not carry a place to put
//! one. What is here is everything that has to be right *before* a provider
//! arrives: identity, provenance, the index, the ordering, and the rule that a
//! vector may never speak for a note the reader has since changed.
//!
//! Keeping the two apart is worth a commit. With a provider fitted in the same
//! phase, a bad answer has two possible authors — the engine or the model — and
//! no way to tell which. Separately, each can be wrong on its own.
//!
//! ## Nothing here knows a vendor
//!
//! No name of any embedding company appears in this file, and no branch reads
//! `space.provider`: it is a label that travels, never a switch that decides.
//! A ranking that behaves differently per vendor is a ranking that changes
//! when the vendor does.

use crate::chunking::{chunk, ChunkId, CHUNKER_VERSION};
use crate::context::MAX_CANDIDATES;
use crate::embedding::{Embedding, EmbeddingRole, EmbeddingSpaceId, SemanticError};
use crate::model::NoteDocument;
use crate::revision::NoteRevision;
use crate::visible_text::visible_text;
use std::collections::BTreeMap;
use uuid::Uuid;

// ------------------------------------------------------------- the provider

/// Whatever turns text into vectors.
///
/// `EmbeddingProvider` is an API and data-minimisation boundary, not a sandbox.
/// Implementations running in-process (such as `LocalProvider` in Phase 4.3C)
/// are trusted code of the Note-it process and hold full process privileges.
/// The narrow interface limits what the Context Engine hands directly to the
/// provider — text chunks rather than paths, filenames, store roots, revisions
/// or write authority — but does not sandbox in-process execution. True process
/// boundary isolation is reserved for optional remote providers in Phase 4.3D,
/// where `noteit-core` communicates over an `AF_UNIX` socket with a separate
/// `noteit-embed` process.
///
/// `embed_document` and `embed_query` are two functions and not one. Not
/// symmetry for its own sake: `multilingual-e5` requires the prefixes
/// `passage: ` and `query: `, other vendors prepend different instructions per
/// input type. A single function would push that knowledge onto every caller,
/// which is exactly the vendor logic this interface exists to contain.
pub trait EmbeddingProvider {
    /// The space this provider's vectors live in.
    fn space(&self) -> EmbeddingSpaceId;

    /// Embeds a batch of chunk texts, in order, one vector each.
    fn embed_document(&self, texts: &[String]) -> Result<Vec<Embedding>, SemanticError>;

    /// Embeds one question.
    fn embed_query(&self, text: &str) -> Result<Embedding, SemanticError>;
}

// --------------------------------------------------------------- the record

/// One vector, and everything needed to know whether it still tells the truth.
///
/// No chunk text, no snippet, no filename and no path. 4.3A weighed keeping the
/// text — it would save a read — and refused: it buys a second place where note
/// content lives and a second way to publish something stale. The snippet comes
/// from the reading the engine already does.
///
/// `source_revision` is **a cache key and nothing else**. It is never published,
/// never reaches an agent and never authorises a write. The shortcut
/// `embedding → revision → write` does not exist; the only chain is still
/// `discover → noteit_read → current revision → decide → write with
/// expected_revision`.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRecord {
    pub note_id: Uuid,
    pub source_revision: NoteRevision,
    pub chunk_id: ChunkId,
    pub chunker_version: u32,
    pub space: EmbeddingSpaceId,
    pub vector: Embedding,
}

/// One note the index thinks is close, before anything has been verified.
///
/// Preliminary is the operative word: it names a note and a chunk of a
/// revision, and every one of those three may already be false. Nothing from
/// here is published until the engine has read the note and agreed.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticHit {
    pub note_id: Uuid,
    pub chunk_id: ChunkId,
    pub source_revision: NoteRevision,
    pub chunker_version: u32,
    pub similarity: f64,
}

// ---------------------------------------------------------------- the index

/// Somewhere to put vectors and ask which are closest.
///
/// A trait because the implementation is meant to be replaceable and, more to
/// the point, **disposable**: an index is derived from notes and can always be
/// rebuilt from them, so losing one costs time and never information.
pub trait SemanticIndex {
    /// The one space this index holds. Mixing spaces is the failure 4.3A
    /// measured, so an index simply does not offer the possibility.
    fn space(&self) -> &EmbeddingSpaceId;

    /// Replaces everything held for one note. An empty batch removes it.
    ///
    /// All or nothing: either every record is accepted or the note is left
    /// exactly as it was. A half-indexed note is an index that looks complete
    /// and is not.
    fn replace_note(
        &mut self,
        note_id: &Uuid,
        records: Vec<EmbeddingRecord>,
    ) -> Result<(), SemanticError>;

    /// Forgets a note. Used when a record turns out to be stale or orphaned;
    /// never touches the note itself.
    fn invalidate_note(&mut self, note_id: &Uuid);

    /// The closest notes to a query vector, best first, at most `limit`.
    ///
    /// **Notes and not chunks.** A note with forty paragraphs must not become
    /// forty candidates, so aggregation happens where the chunks are: each note
    /// is represented by its best-scoring chunk, and which chunk that was
    /// travels along so the snippet can be rebuilt from it later.
    fn nearest_notes(
        &self,
        query: &Embedding,
        limit: usize,
    ) -> Result<Vec<SemanticHit>, SemanticError>;

    /// How many vectors are held. Diagnostics, never published.
    fn vector_count(&self) -> usize;
}

/// The whole index, in memory, compared one vector at a time.
///
/// Brute force, and measured before being chosen: 4.3A timed 3.5 ms against ten
/// thousand vectors. Approximate nearest neighbours become worth their
/// complexity somewhere past fifty milliseconds, which is hundreds of thousands
/// of vectors — several orders of magnitude beyond any note store this
/// application has seen. Nothing is written to disk.
#[derive(Debug)]
pub struct InMemoryIndex {
    space: EmbeddingSpaceId,
    /// Ordered, so that iteration is the same on every run. A `HashMap` here
    /// would make ties depend on a hash seed, which is the same class of defect
    /// as letting the filesystem decide an order.
    by_note: BTreeMap<Uuid, Vec<EmbeddingRecord>>,
}

impl InMemoryIndex {
    pub fn new(space: EmbeddingSpaceId) -> Self {
        Self {
            space,
            by_note: BTreeMap::new(),
        }
    }

    pub fn notes(&self) -> usize {
        self.by_note.len()
    }

    /// Whether anything is held for this note.
    ///
    /// The whole of the incremental lifecycle rests on this question, and on
    /// the fact that the answer becomes `false` on its own: a record whose
    /// revision no longer matches is invalidated by the engine during a
    /// retrieval, so "the index does not hold it" is also how "the note
    /// changed" is discovered. That leaves the canonical revision as the only
    /// detector of note state, which is the rule §7 of the specification
    /// exists to protect — a second, cheaper detector would disagree with it
    /// eventually, and disagree silently.
    pub fn holds(&self, note_id: &Uuid) -> bool {
        self.by_note.contains_key(note_id)
    }

    /// Every note the index holds something for, in a stable order.
    pub fn note_ids(&self) -> Vec<Uuid> {
        self.by_note.keys().copied().collect()
    }
}

impl SemanticIndex for InMemoryIndex {
    fn space(&self) -> &EmbeddingSpaceId {
        &self.space
    }

    fn replace_note(
        &mut self,
        note_id: &Uuid,
        records: Vec<EmbeddingRecord>,
    ) -> Result<(), SemanticError> {
        // Validate the whole batch before touching anything. Rejecting halfway
        // through would leave the note holding some of an update, which is the
        // one state that cannot be told apart from a correct one later.
        for record in &records {
            if record.note_id != *note_id {
                return Err(SemanticError::InvalidResponse);
            }
            if record.space != self.space || record.vector.space() != &self.space {
                return Err(SemanticError::SpaceMismatch);
            }
            if record.chunker_version != CHUNKER_VERSION {
                return Err(SemanticError::ChunkerMismatch {
                    expected: CHUNKER_VERSION,
                    actual: record.chunker_version,
                });
            }
        }

        if records.is_empty() {
            self.by_note.remove(note_id);
        } else {
            self.by_note.insert(*note_id, records);
        }
        Ok(())
    }

    fn invalidate_note(&mut self, note_id: &Uuid) {
        self.by_note.remove(note_id);
    }

    fn nearest_notes(
        &self,
        query: &Embedding,
        limit: usize,
    ) -> Result<Vec<SemanticHit>, SemanticError> {
        if query.space() != &self.space {
            return Err(SemanticError::SpaceMismatch);
        }

        // The space was compared once, above, and every record in this index
        // was refused on insert unless it declared exactly this space — see
        // `replace_note`, which validates the whole batch before touching
        // anything. So query and record are in the same space by construction,
        // and the comparison below is between *vectors*, which still refuses a
        // dimension it does not recognise.
        //
        // This is a measured difference and not a tidy-up. `embedding::cosine`
        // compares two `EmbeddingSpaceId`s, and one of those holds a provider
        // name, a model name and a sixty-four character digest: doing it per
        // record turned a search over twenty thousand vectors into twenty
        // thousand string comparisons, and cost 19.8 ms where the arithmetic
        // costs 3. What is *not* skipped is the check that matters — the space
        // is still compared, once, against the query.
        let query_vector = query.vector();
        let mut best: Vec<SemanticHit> = Vec::with_capacity(self.by_note.len());
        for records in self.by_note.values() {
            let mut winner: Option<SemanticHit> = None;
            for record in records {
                let similarity = query_vector.cosine(record.vector.vector())?;
                let better = match &winner {
                    None => true,
                    // A note's own chunks tie by `chunk_id`, so which paragraph
                    // represents a note is the same answer on every run.
                    Some(current) => match similarity.total_cmp(&current.similarity) {
                        std::cmp::Ordering::Greater => true,
                        std::cmp::Ordering::Equal => record.chunk_id < current.chunk_id,
                        std::cmp::Ordering::Less => false,
                    },
                };
                if better {
                    winner = Some(SemanticHit {
                        note_id: record.note_id,
                        chunk_id: record.chunk_id.clone(),
                        source_revision: record.source_revision.clone(),
                        chunker_version: record.chunker_version,
                        similarity,
                    });
                }
            }
            if let Some(hit) = winner {
                best.push(hit);
            }
        }

        // `total_cmp` and never `partial_cmp().unwrap()`: the second panics on
        // a `NaN` that the vector type already makes impossible, which is
        // exactly the kind of guarantee that stops being true one refactor
        // later. The identifier closes the order so it is total.
        best.sort_by(|left, right| {
            right
                .similarity
                .total_cmp(&left.similarity)
                .then_with(|| left.note_id.cmp(&right.note_id))
        });
        best.truncate(limit);
        Ok(best)
    }

    fn vector_count(&self) -> usize {
        self.by_note.values().map(Vec::len).sum()
    }
}

// -------------------------------------------------------------- indexing

/// Turns one note into records and puts them in the index.
///
/// Reading, from end to end. Nothing here writes a note, moves a timestamp or
/// touches the file: `updated_at` took a whole phase to stop moving when a note
/// was merely opened, and indexing is not going to undo that.
///
/// The batch is atomic. A provider that answers five chunks with four vectors
/// has not answered — the count is the only thing that says which vector
/// belongs to which chunk, and a batch that lines up by accident would attach
/// paragraph three's meaning to paragraph four's identity. Nothing is indexed
/// and the previous state stays coherent.
pub fn index_document(
    document: &NoteDocument,
    provider: &dyn EmbeddingProvider,
    index: &mut dyn SemanticIndex,
) -> Result<usize, SemanticError> {
    let space = provider.space();
    if &space != index.space() {
        return Err(SemanticError::SpaceMismatch);
    }

    let note_id = document.metadata.id;
    let revision = NoteRevision::for_document(document).map_err(|_| SemanticError::Unindexable)?;
    let visible = visible_text(&document.content);
    let chunks = chunk(&visible);

    if chunks.is_empty() {
        // An empty note has nothing to be about, and whatever was held for it
        // was about a version that had something.
        index.replace_note(&note_id, Vec::new())?;
        return Ok(0);
    }

    let texts: Vec<String> = chunks.iter().map(|piece| piece.text.clone()).collect();
    let vectors = provider.embed_document(&texts)?;
    if vectors.len() != texts.len() {
        return Err(SemanticError::InvalidResponse);
    }

    let mut records = Vec::with_capacity(chunks.len());
    for (piece, vector) in chunks.iter().zip(vectors) {
        // Checked at the boundary, on what the provider actually returned and
        // not on what it advertises. A provider that answered from a different
        // model is caught here rather than ranking.
        if vector.space() != &space {
            return Err(SemanticError::SpaceMismatch);
        }
        if vector.role() != EmbeddingRole::Document {
            return Err(SemanticError::InvalidResponse);
        }
        records.push(EmbeddingRecord {
            note_id,
            source_revision: revision.clone(),
            chunk_id: ChunkId::of(
                &note_id,
                &revision,
                piece.ordinal,
                CHUNKER_VERSION,
                &piece.text,
            )?,
            chunker_version: CHUNKER_VERSION,
            space: space.clone(),
            vector,
        });
    }

    let indexed = records.len();
    index.replace_note(&note_id, records)?;
    Ok(indexed)
}

// -------------------------------------------------------------- the runtime

/// What the semantic channel does when it cannot answer.
///
/// Two states and no third, because the third — "tried, failed, said nothing" —
/// is the one that lies to whoever asked for semantics on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticFallback {
    /// Degrade to the lexical answer. The default everywhere.
    Automatic,
    /// Refuse instead of degrading. For a caller who asked for the semantic
    /// channel and needs to know it did not get it.
    Required,
}

/// The ceilings the semantic channel obeys.
///
/// Policy of the runtime, deliberately not a constant buried in the engine.
/// 4.3A proposed at most three purely semantic candidates and 4.3A.R1.2 did not
/// freeze the number, so it lives somewhere it can be argued with rather than
/// as a `3` in the middle of a sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticPolicy {
    /// How many candidates the semantic channel may contribute **on its own** —
    /// that is, notes nothing lexical admitted.
    ///
    /// A limit exists because a nearest-neighbour search always has a nearest
    /// neighbour. Today the engine answers an unmatched question with nothing,
    /// which is true information; ten confident-looking strangers would be
    /// worse than that silence, so "I found nothing in your words" stays
    /// legible.
    pub max_semantic_only: usize,
    /// How many preliminary hits to ask the index for, before provenance has
    /// thrown any away. Larger than `max_semantic_only` because validation
    /// discards, and a channel that asked for exactly three and lost two to
    /// staleness would publish one.
    pub preliminary_hits: usize,
}

impl Default for SemanticPolicy {
    fn default() -> Self {
        Self {
            max_semantic_only: 3,
            preliminary_hits: MAX_CANDIDATES,
        }
    }
}

/// A provider, an index and the policy binding them, for one retrieval.
///
/// Borrowed rather than owned, and the index mutably, because validating
/// provenance is allowed to *forget* — a record about a revision that no longer
/// exists is invalidated on the spot rather than rediscovered on every query.
pub struct SemanticRuntime<'a> {
    pub provider: &'a dyn EmbeddingProvider,
    pub index: &'a mut dyn SemanticIndex,
    pub fallback: SemanticFallback,
    pub policy: SemanticPolicy,
}

impl<'a> SemanticRuntime<'a> {
    /// The ordinary shape: degrade quietly, default ceilings.
    pub fn new(provider: &'a dyn EmbeddingProvider, index: &'a mut dyn SemanticIndex) -> Self {
        Self {
            provider,
            index,
            fallback: SemanticFallback::Automatic,
            policy: SemanticPolicy::default(),
        }
    }

    pub fn requiring_semantics(mut self) -> Self {
        self.fallback = SemanticFallback::Required;
        self
    }

    /// The policy the caller resolved from configuration.
    pub fn with_fallback(mut self, fallback: SemanticFallback) -> Self {
        self.fallback = fallback;
        self
    }

    pub fn with_policy(mut self, policy: SemanticPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Embeds the question and asks the index, checking the provider's answer
    /// at the boundary.
    pub(crate) fn preliminary_hits(
        &self,
        question: &str,
    ) -> Result<Vec<SemanticHit>, SemanticError> {
        let embedded = self.provider.embed_query(question)?;
        // What the provider *returned*, not what it advertises. A provider that
        // answers a query in another space is the failure 4.3A measured, and
        // the only place to catch it is where the answer arrives.
        if embedded.space() != &self.provider.space() {
            return Err(SemanticError::SpaceMismatch);
        }
        if embedded.role() != EmbeddingRole::Query {
            return Err(SemanticError::InvalidResponse);
        }
        self.index
            .nearest_notes(&embedded, self.policy.preliminary_hits)
    }
}
