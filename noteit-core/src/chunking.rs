//! Cutting a note into the pieces a vector can be about.
//!
//! Measured in 4.3A, and it is why this file exists: the corpus's long note —
//! 7 878 characters with the relevant passage in the middle — is *lost* by
//! embedding the whole note and *found* by embedding it a paragraph at a time.
//! One vector for a long note is an average of everything it says, and an
//! average of everything is about nothing.
//!
//! The chunker reads and never writes. It is a derived view of a note, with a
//! version of its own that enters both the identity of a chunk and the validity
//! of anything cached about one — so changing how notes are cut invalidates the
//! vectors made under the old cut instead of silently mixing the two.
//!
//! Its input is the **visible text** (`visible_text`), the same projection the
//! lexical side searches: colour, HTML comment and front matter are not
//! embedded for the same reason they are not searchable.

use crate::embedding::{canonical_object, CanonicalError, CanonicalValue};
use crate::hashing::sha256_hex;
use crate::revision::NoteRevision;
use uuid::Uuid;

/// The version of the cut below.
///
/// Bumped whenever the boundaries move. It is part of [`ChunkId`] and part of
/// what makes an [`crate::semantic::EmbeddingRecord`] stale, so a vector made
/// under an older cut can never be mistaken for one made under this.
pub const CHUNKER_VERSION: u32 = 1;

/// The size a chunk aims at, in characters.
///
/// Characters and not bytes, because a limit in bytes is a different limit for
/// Portuguese than for English, and because a cut on a character boundary
/// cannot split one.
pub const MAX_CHUNK_CHARS: usize = 800;

/// The prefix every chunk identity is hashed under. See
/// [`crate::embedding::ARTIFACT_DOMAIN`] for what a domain separator does and,
/// as importantly, what it does not promise.
pub const CHUNK_DOMAIN: &str = "noteit.chunk.v1\n";

/// One piece of a note's visible text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Its position in the note, from zero, in reading order.
    pub ordinal: u32,
    /// Where it begins in the visible text it was cut from — the way back to a
    /// snippet in the note rather than a snippet of a copy.
    pub at: usize,
    pub text: String,
}

/// Cuts visible text into chunks.
///
/// 1. paragraphs are separated by blank lines — the boundary Markdown already
///    uses and the one the author chose;
/// 2. a paragraph of at most [`MAX_CHUNK_CHARS`] characters is one chunk;
/// 3. a longer one is cut at the last sentence boundary that fits, else at the
///    last whitespace that fits, else exactly at the limit;
/// 4. no overlap. Overlap multiplies vectors to recover context an average has
///    already blurred: a certain cost for an unmeasured gain;
/// 5. an empty note produces no chunks;
/// 6. order is preserved and the whole thing is a pure function of its input.
pub fn chunk(visible: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for (at, paragraph) in paragraphs(visible) {
        let mut offset = at;
        let mut rest = paragraph;
        while !rest.is_empty() {
            let cut = split_point(rest, MAX_CHUNK_CHARS);
            debug_assert!(cut > 0, "a cut of zero would never finish the paragraph");
            let (head, tail) = rest.split_at(cut);
            let trimmed = head.trim();
            if !trimmed.is_empty() {
                let lead = head.len() - head.trim_start().len();
                chunks.push(Chunk {
                    ordinal: chunks.len() as u32,
                    at: offset + lead,
                    text: trimmed.to_string(),
                });
            }
            offset += cut;
            let after = tail.trim_start();
            offset += tail.len() - after.len();
            rest = after;
        }
    }
    chunks
}

/// The paragraphs of visible text, each with where it starts.
///
/// A line is a separator when it has nothing on it. Slices of the input rather
/// than rebuilt strings, so an offset here is an offset a snippet can be taken
/// around — the point being that the snippet comes from the note and not from a
/// copy of it that could have been normalised on the way.
fn paragraphs(visible: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut at = 0;

    for line in visible.split_inclusive('\n') {
        let blank = line.trim().is_empty();
        if blank {
            if let Some(from) = start.take() {
                found.push((from, visible[from..end].trim_end()));
            }
        } else {
            if start.is_none() {
                start = Some(at);
            }
            end = at + line.len();
        }
        at += line.len();
    }
    if let Some(from) = start {
        found.push((from, visible[from..end].trim_end()));
    }

    found.retain(|(_, text)| !text.is_empty());
    found
}

/// Where to cut a piece of text so that at most `limit` characters come off.
///
/// Three rules, tried in order, so that there is no text for which the answer
/// is undefined:
///
/// 1. the last sentence boundary at or before the limit;
/// 2. failing that, the last whitespace at or before it;
/// 3. failing that, exactly at the limit.
///
/// A sentence boundary is the position just after a `.`, `!` or `?` that is
/// followed by whitespace. Nothing cleverer: an abbreviation ends a chunk early,
/// which costs one chunk, while a real sentence splitter costs a dependency, a
/// language and a table of exceptions — and would still be wrong about
/// abbreviations.
///
/// The returned index is always on a character boundary, so rule 3 cannot split
/// a multi-byte character.
fn split_point(text: &str, limit: usize) -> usize {
    let mut sentence: Option<usize> = None;
    let mut space: Option<usize> = None;
    let mut hard = text.len();
    let mut counted = 0;
    let mut previous: Option<char> = None;

    for (index, character) in text.char_indices() {
        if counted == limit {
            hard = index;
            break;
        }
        if character.is_whitespace() && index > 0 {
            if matches!(previous, Some('.') | Some('!') | Some('?')) {
                sentence = Some(index);
            }
            space = Some(index);
        }
        previous = Some(character);
        counted += 1;
    }

    if counted < limit {
        return text.len();
    }
    sentence.or(space).unwrap_or(hard)
}

/// A chunk's identity: which note, at which version, in which position, saying
/// what, cut by which chunker.
///
/// Every component is fixed-length or a number, and they go through the one
/// canonical encoding in the crate rather than into a `format!`. Concatenating
/// variable-length components is ambiguous — two different sets of components
/// can make one byte string, and then two chunks share an identity — which is
/// the same defect class the artifact manifest exists to avoid.
///
/// The text digest is in there as well as the position, so that two identical
/// paragraphs in different notes do not collide and a paragraph that moved is
/// not mistaken for the one that used to be there. The revision would usually
/// have moved too; "usually" is not a property worth relying on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId(String);

impl ChunkId {
    pub fn of(
        note_id: &Uuid,
        source_revision: &NoteRevision,
        ordinal: u32,
        chunker_version: u32,
        text: &str,
    ) -> Result<Self, CanonicalError> {
        let text_digest = sha256_hex(text.as_bytes());
        let note = note_id.hyphenated().to_string();
        let encoded = canonical_object(&[
            ("chunk_sha256", CanonicalValue::Token(&text_digest)),
            (
                "chunker_version",
                CanonicalValue::Number(u64::from(chunker_version)),
            ),
            ("note_id", CanonicalValue::Token(&note)),
            ("ordinal", CanonicalValue::Number(u64::from(ordinal))),
            (
                "source_revision",
                CanonicalValue::Token(source_revision.as_str()),
            ),
        ])?;
        let mut input = String::with_capacity(CHUNK_DOMAIN.len() + encoded.len());
        input.push_str(CHUNK_DOMAIN);
        input.push_str(&encoded);
        Ok(Self(sha256_hex(input.as_bytes())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NoteDocument;
    use crate::visible_text::visible_text;

    fn texts(visible: &str) -> Vec<String> {
        chunk(visible).into_iter().map(|piece| piece.text).collect()
    }

    fn note_of(body: &str) -> (Uuid, NoteRevision) {
        let mut document = NoteDocument::new_empty();
        document.content = body.to_string();
        let revision = NoteRevision::for_document(&document).expect("revision");
        (document.metadata.id, revision)
    }

    // ------------------------------------------------------- the boundaries

    #[test]
    fn an_empty_note_produces_no_chunks() {
        assert!(chunk("").is_empty());
        assert!(chunk("   \n\n  \n").is_empty());
    }

    #[test]
    fn one_short_paragraph_is_one_chunk() {
        assert_eq!(
            texts("Apneia obstrutiva do sono."),
            ["Apneia obstrutiva do sono."]
        );
    }

    #[test]
    fn a_blank_line_separates_and_several_of_them_separate_once() {
        assert_eq!(texts("primeiro\n\nsegundo"), ["primeiro", "segundo"]);
        assert_eq!(
            texts("primeiro\n\n\n\n   \n\nsegundo"),
            ["primeiro", "segundo"]
        );
    }

    #[test]
    fn the_lines_of_one_paragraph_stay_together() {
        assert_eq!(
            texts("uma linha\noutra linha\n\nsegundo parágrafo"),
            ["uma linha\noutra linha", "segundo parágrafo"]
        );
    }

    #[test]
    fn carriage_returns_do_not_survive_the_projection() {
        // The chunker's input is `visible_text`, which already normalises line
        // endings, so this is the shape a CRLF note really arrives in.
        assert_eq!(
            texts(&visible_text("primeiro\r\n\r\nsegundo\r\n")),
            ["primeiro", "segundo"]
        );
    }

    #[test]
    fn order_is_the_notes_order_and_the_ordinals_say_so() {
        let chunks = chunk("um\n\ndois\n\ntrês");
        assert_eq!(
            chunks.iter().map(|piece| piece.ordinal).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(
            chunks
                .iter()
                .map(|piece| piece.text.as_str())
                .collect::<Vec<_>>(),
            ["um", "dois", "três"]
        );
    }

    #[test]
    fn every_chunk_knows_where_it_came_from() {
        let visible = "primeiro parágrafo\n\nsegundo parágrafo";
        for piece in chunk(visible) {
            assert!(
                visible[piece.at..].starts_with(&piece.text),
                "the offset must land on the chunk's own first character"
            );
        }
    }

    // ------------------------------------------------------------ the limit

    #[test]
    fn a_paragraph_exactly_at_the_limit_is_not_cut() {
        let body = "a".repeat(MAX_CHUNK_CHARS);
        assert_eq!(texts(&body), [body]);
    }

    #[test]
    fn one_character_past_the_limit_is_cut_and_nothing_is_lost() {
        let body = "a".repeat(MAX_CHUNK_CHARS + 1);
        let pieces = texts(&body);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].chars().count(), MAX_CHUNK_CHARS);
        assert_eq!(pieces[1].chars().count(), 1);
        assert_eq!(pieces.concat(), body);
    }

    #[test]
    fn a_sentence_boundary_is_preferred_to_a_word_boundary() {
        let head = format!("{}. ", "a".repeat(MAX_CHUNK_CHARS - 200));
        let tail = "palavra ".repeat(60);
        let pieces = texts(&format!("{head}{tail}"));
        assert!(
            pieces[0].ends_with('.'),
            "cut after the sentence, not mid-clause"
        );
        assert_eq!(pieces[0].chars().count(), MAX_CHUNK_CHARS - 199);
    }

    #[test]
    fn a_word_boundary_is_preferred_to_a_hard_cut() {
        let body = "palavra ".repeat(200);
        let pieces = texts(&body);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(
                piece.split_whitespace().all(|word| word == "palavra"),
                "no word may be cut in half: {piece}"
            );
        }
    }

    #[test]
    fn text_with_no_whitespace_at_all_is_cut_at_the_limit() {
        let body = "x".repeat(MAX_CHUNK_CHARS * 3 + 7);
        let pieces = texts(&body);
        assert_eq!(pieces.len(), 4);
        assert_eq!(pieces.concat(), body);
    }

    #[test]
    fn a_hard_cut_never_splits_a_character() {
        // Four-byte characters, so a cut counted in bytes would land inside one
        // and the string would not be valid UTF-8 to build at all.
        let body = "🙂".repeat(MAX_CHUNK_CHARS * 2 + 5);
        let pieces = texts(&body);
        assert_eq!(pieces.concat(), body);
        for piece in &pieces {
            assert!(piece.chars().all(|character| character == '🙂'));
        }
    }

    #[test]
    fn accents_count_as_one_character_each() {
        let body = "á".repeat(MAX_CHUNK_CHARS);
        assert_eq!(texts(&body), [body]);
    }

    #[test]
    fn markdown_and_task_lists_are_ordinary_text_here() {
        let visible = visible_text("# Título\n\n- [ ] reler hipertensão\n- [x] feito\n\nfim");
        let pieces = texts(&visible);
        assert_eq!(pieces.len(), 3);
        assert!(pieces[1].contains("reler hipertensão"));
        assert_eq!(pieces[2], "fim");
    }

    #[test]
    fn a_very_large_note_is_cut_into_bounded_pieces_and_stays_deterministic() {
        let body = "Uma frase razoavelmente longa sobre alguma coisa. ".repeat(4000);
        let once = chunk(&body);
        let again = chunk(&body);
        assert_eq!(once, again);
        assert!(once.len() > 100);
        for piece in &once {
            assert!(piece.text.chars().count() <= MAX_CHUNK_CHARS);
        }
    }

    #[test]
    fn nothing_overlaps_and_the_order_can_be_reconstructed() {
        let body = "Primeiro parágrafo com bastante coisa. Segunda frase dele.\n\n\
                    Segundo parágrafo.\n\nTerceiro.";
        let pieces = chunk(body);
        let mut previous_end = 0;
        for piece in &pieces {
            assert!(
                piece.at >= previous_end,
                "a chunk may not start before the previous one ended"
            );
            previous_end = piece.at + piece.text.len();
        }
        assert_eq!(
            pieces.iter().map(|piece| piece.ordinal).collect::<Vec<_>>(),
            (0..pieces.len() as u32).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------- the identity

    #[test]
    fn the_same_chunk_of_the_same_revision_is_the_same_identity() {
        let (note, revision) = note_of("um corpo");
        let one = ChunkId::of(&note, &revision, 0, CHUNKER_VERSION, "texto").expect("id");
        let again = ChunkId::of(&note, &revision, 0, CHUNKER_VERSION, "texto").expect("id");
        assert_eq!(one, again);
    }

    #[test]
    fn changing_any_component_changes_the_chunk_identity() {
        let (note, revision) = note_of("um corpo");
        let (other_note, other_revision) = note_of("outro corpo");
        let base = ChunkId::of(&note, &revision, 0, CHUNKER_VERSION, "texto").expect("id");

        assert_ne!(
            ChunkId::of(&other_note, &revision, 0, CHUNKER_VERSION, "texto").expect("id"),
            base
        );
        assert_ne!(
            ChunkId::of(&note, &other_revision, 0, CHUNKER_VERSION, "texto").expect("id"),
            base
        );
        assert_ne!(
            ChunkId::of(&note, &revision, 1, CHUNKER_VERSION, "texto").expect("id"),
            base
        );
        assert_ne!(
            ChunkId::of(&note, &revision, 0, CHUNKER_VERSION + 1, "texto").expect("id"),
            base
        );
        assert_ne!(
            ChunkId::of(&note, &revision, 0, CHUNKER_VERSION, "outro texto").expect("id"),
            base
        );
    }

    #[test]
    fn two_notes_with_the_same_paragraph_do_not_share_a_chunk_identity() {
        let (left, left_revision) = note_of("mesmo parágrafo");
        let (right, right_revision) = note_of("mesmo parágrafo");
        assert_ne!(left, right);
        assert_ne!(
            ChunkId::of(&left, &left_revision, 0, CHUNKER_VERSION, "mesmo parágrafo").expect("id"),
            ChunkId::of(
                &right,
                &right_revision,
                0,
                CHUNKER_VERSION,
                "mesmo parágrafo"
            )
            .expect("id")
        );
    }
}
