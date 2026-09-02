//! Finding text across every note.
//!
//! This module knows about strings and note identifiers, and nothing else. It
//! does not open files, does not know GTK exists, does not know a WebView
//! exists, and holds no state. That is deliberate: the interesting part of
//! search — what counts as a match, what a result looks like, how the list is
//! ordered — is exactly the part a future command line would need, and burying
//! it in a WebView handler would mean writing it twice. Phase 4 can call
//! [`search_notes`] without this file changing.
//!
//! There is **no index**. Notes are read, folded and scanned every time. For
//! the number of notes a person keeps that is far below the point where an
//! index would pay for itself, and an index is not free: it has to be
//! invalidated, rebuilt after an external edit, versioned, backed up and
//! recovered after a crash. See ADR-027 for the measurements behind that.

use crate::visible_text::visible_text;
use crate::warning::ReadBatch;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ceilings on the question and on the answer. None of them is a limit anybody
/// writing a note will meet.
///
/// What they bound is precisely this: how long a query may be, how many notes
/// come back, and how much of a note is quoted around a match. They do **not**
/// bound the note. A note is a text file and anything at all can be pasted
/// into one, and every byte of it is folded and scanned — which is the
/// contract: search finds text at the end of a large note, so it has to read
/// to the end of a large note. A single note far beyond anything a person
/// writes therefore costs what its size costs, and the honest statement is
/// that this has been measured rather than that it is bounded. See ADR-027.
pub const MAX_QUERY_CHARS: usize = 512;
pub const MAX_RESULTS: usize = 100;
/// How much of the note is shown around a match.
pub const MAX_SNIPPET_CHARS: usize = 240;
/// How much of the first line is shown as the note's name.
pub const MAX_LABEL_CHARS: usize = 120;

/// What a note is called when it has nothing in it to be called after.
pub const EMPTY_LABEL: &str = "Nota vazia";

/// One note that matched, as the interface receives it.
///
/// Serialized straight onto the bridge: there is no second shape to keep in
/// step, and the page is handed identifiers and text rather than anything it
/// could turn into a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// The note this came from. Every action the interface offers is addressed
    /// by this, never by a path and never by the label.
    pub note_id: Uuid,
    /// A name derived from the note's first line. Never written to the file.
    pub label: String,
    /// The text around the first match, or the opening of the note when the
    /// query is empty.
    pub snippet: String,
    /// How many times the query occurs in the note. Zero for a listing.
    pub match_count: usize,
    /// The first occurrence **as the note spells it**, or empty for a listing.
    ///
    /// Global search folds accents, so `biopsia` finds `Biópsia`; the editor's
    /// own find does not, because it is what Replace acts on and a replacement
    /// that quietly rewrites accented words is the kind of surprise this
    /// project does not ship. Carrying the spelling that actually matched is
    /// what lets the note being opened highlight it under the stricter rule.
    pub matched_text: String,
}

/// A string folded for comparison.
///
/// Folding changes length — `Á` becomes `a`, a combining accent disappears
/// entirely — so an offset in the folded text is not an offset in the source.
/// The way back is [`source_offset_of`], computed only for the note a match
/// was actually found in: keeping a byte-for-byte map alongside every folded
/// note costs eight bytes per byte of text, on every note, on every keystroke,
/// to answer a question asked about almost none of them.
#[derive(Debug, Clone)]
pub struct Folded {
    pub text: String,
}

impl Folded {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Where a byte offset in `fold(source).text` came from in `source`.
///
/// Walks the source once, folding as it goes, which is the same work `fold`
/// did — but only for a note that matched, and only up to the match.
fn source_offset_of(source: &str, folded_offset: usize) -> usize {
    if folded_offset == 0 {
        return 0;
    }

    let mut folded_len = 0;
    for (index, character) in source.char_indices() {
        if folded_len >= folded_offset {
            return index;
        }
        for lowered in character.to_lowercase() {
            if let Some(mapped) = fold_char(lowered) {
                folded_len += mapped.len_utf8();
            }
        }
    }
    source.len()
}

/// Folds one character for comparison, or drops it.
///
/// Two things happen here and only two: case is removed, and a Latin letter
/// loses its diacritic. `None` means the character contributes nothing — which
/// is how a combining mark is handled, so text that arrives decomposed
/// (`o` followed by U+0301) folds exactly like the precomposed `ó`.
///
/// The table covers Latin-1 Supplement and Latin Extended-A, which is every
/// accented letter Portuguese uses and most of what the rest of Latin-script
/// Europe uses. It is written out rather than obtained from a Unicode
/// normalisation crate because that is a dependency, a transitive dependency
/// and a table of every script on earth, in exchange for letters this
/// application's readers do not type. A precomposed character outside those
/// blocks — Vietnamese `ế`, for instance — folds only if it arrives
/// decomposed. That limit is documented rather than hidden, and widening it is
/// adding rows here.
fn fold_char(character: char) -> Option<char> {
    // Combining Diacritical Marks: present only in decomposed text, and
    // meaningless once the letter they sit on has been folded.
    if ('\u{0300}'..='\u{036F}').contains(&character) {
        return None;
    }

    Some(match character {
        'a'..='z' | '0'..='9' => character,
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    })
}

/// Folds a string for comparison.
///
/// Case first, through `char::to_lowercase`, which knows the Unicode rules and
/// may produce more than one character; then the diacritic table above.
pub fn fold(text: &str) -> Folded {
    let mut folded = String::with_capacity(text.len());

    for character in text.chars() {
        for lowered in character.to_lowercase() {
            if let Some(mapped) = fold_char(lowered) {
                folded.push(mapped);
            }
        }
    }

    Folded { text: folded }
}

/// The query as it will be compared, or `None` when there is nothing to look
/// for. An over-long query is refused rather than truncated: answering a
/// question nobody asked is worse than answering none.
pub fn prepare_query(query: &str) -> Option<Folded> {
    if query.chars().count() > MAX_QUERY_CHARS {
        return None;
    }
    let folded = fold(query.trim());
    if folded.is_empty() {
        return None;
    }
    Some(folded)
}

/// A name for the note, taken from its first line with something on it.
///
/// Note-it has no title field and is not about to grow one: a title would be
/// content the reader did not write. The first line is what a person would
/// point at if asked which note this is, so that is what the list shows — as
/// the reader sees that line, not as the file spells it.
pub fn label_for(content: &str) -> String {
    label_of_visible(&visible_text(content))
}

/// The same, for a caller that has already projected the note.
fn label_of_visible(visible: &str) -> String {
    match visible.lines().map(str::trim).find(|line| !line.is_empty()) {
        None => EMPTY_LABEL.to_string(),
        Some(line) => truncate_chars(line, MAX_LABEL_CHARS),
    }
}

fn truncate_chars(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        None => text.to_string(),
        Some((cut, _)) => format!("{}…", &text[..cut]),
    }
}

/// A window of the note around the match, on character boundaries.
///
/// A third of the budget goes in front of the match and the rest follows it,
/// and the whole window is measured from where it starts — so a match longer
/// than the budget is cut rather than pushing the snippet past it.
fn snippet_around(content: &str, from: usize) -> String {
    let lead = MAX_SNIPPET_CHARS / 3;

    let mut start = from;
    let mut before = 0;
    for (index, _) in content[..from].char_indices().rev() {
        start = index;
        before += 1;
        if before == lead {
            break;
        }
    }

    let mut end = start;
    for (offset, character) in content[start..].char_indices().take(MAX_SNIPPET_CHARS) {
        end = start + offset + character.len_utf8();
    }

    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    // A snippet is one line's worth of context, so the line breaks inside it
    // become spaces rather than turning a result row into a paragraph.
    snippet.extend(content[start..end].chars().map(|character| {
        if character == '\n' || character == '\r' {
            ' '
        } else {
            character
        }
    }));
    if end < content.len() {
        snippet.push('…');
    }

    snippet.trim().to_string()
}

/// The opening of a note, as a list that has nothing to match on shows it.
///
/// The same window an empty query gives a result, so a note looks the same
/// wherever it is listed — the search palette and the trash included.
pub fn opening_of(content: &str) -> String {
    snippet_around(&visible_text(content), 0)
}

/// Counts every occurrence and returns where the first one starts.
fn locate(haystack: &Folded, needle: &str) -> Option<(usize, usize)> {
    let mut count = 0;
    let mut first = None;
    let mut from = 0;

    while let Some(found) = haystack.text[from..].find(needle) {
        let start = from + found;
        if first.is_none() {
            first = Some(start);
        }
        count += 1;
        // Overlapping occurrences are still occurrences, but stepping by one
        // byte would split a character; stepping by the needle keeps the count
        // to the non-overlapping reading everyone expects from a find.
        from = start + needle.len();
    }

    first.map(|start| (start, count))
}

/// Searches one note, returning a result only when the query occurs in it.
pub fn search_note(query: &Folded, note_id: Uuid, content: &str) -> Option<SearchResult> {
    // Everything below happens on the note as the reader sees it, which is
    // what makes `data-note-it-color` unfindable: the attribute is not in the
    // text, so there is nothing to match. It also makes `matched_text` a
    // phrase the editor's own find can locate, since that searches the
    // document rather than the file.
    let visible = visible_text(content);
    let folded = fold(&visible);
    let (start, count) = locate(&folded, &query.text)?;
    let from = source_offset_of(&visible, start);
    let to = source_offset_of(&visible, start + query.text.len());

    Some(SearchResult {
        note_id,
        label: label_of_visible(&visible),
        snippet: snippet_around(&visible, from),
        match_count: count,
        matched_text: visible[from..to.max(from)].to_string(),
    })
}

/// What the palette is shown after one query.
///
/// The notice exists so that an empty list is never the whole answer. A store
/// that could not be scanned and a store with nothing matching produce the
/// same empty `results`, and only this tells a reader which one happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchAnswer {
    pub results: Vec<SearchResult>,
    /// A short line for the palette's status, or `None` when the scan was
    /// wholly healthy.
    pub notice: Option<String>,
}

/// Turns what the store answered into what the palette says.
///
/// The runtime and the tests both go through here, so "an error must not look
/// like no results" is one decision in one place rather than a convention the
/// interface is trusted to keep.
pub fn resolve_search_answer(outcome: Result<ReadBatch<SearchResult>, String>) -> SearchAnswer {
    match outcome {
        Ok(batch) if batch.warnings.is_empty() => SearchAnswer {
            results: batch.items,
            notice: None,
        },
        Ok(batch) => {
            let unreadable = batch.warnings.len();
            SearchAnswer {
                results: batch.items,
                notice: Some(format!(
                    "{unreadable} nota(s) não puderam ser lidas; os resultados estão incompletos"
                )),
            }
        }
        Err(error) => SearchAnswer {
            results: Vec::new(),
            notice: Some(format!("a busca falhou: {error}")),
        },
    }
}

/// Every note the query occurs in, in the order they were handed over.
///
/// Every note is asked: the caller hands over the whole store, and what stops
/// early here is the accumulation of *results*, never the scan of notes still
/// unseen. Nothing else would let the interface say it searches every note.
///
/// The caller decides the order, and today that is the store's own recency
/// rule — the same one that decides which note a summon brings back, so a
/// reader meets one idea of "most recent" everywhere. Ranking is deliberately
/// nothing more than that: a score nobody can explain is a list nobody can
/// predict.
pub fn search_notes<'a, I>(query: &str, notes: I) -> Vec<SearchResult>
where
    I: IntoIterator<Item = (Uuid, &'a str)>,
{
    let Some(prepared) = prepare_query(query) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for (note_id, content) in notes {
        if results.len() >= MAX_RESULTS {
            break;
        }
        if let Some(result) = search_note(&prepared, note_id, content) {
            results.push(result);
        }
    }
    results
}

/// The notes themselves, with no query: the list an empty search box shows.
///
/// An empty box that showed nothing would be a dead end; showing the most
/// recent notes makes the same control a way to move between them.
pub fn recent_notes<'a, I>(notes: I) -> Vec<SearchResult>
where
    I: IntoIterator<Item = (Uuid, &'a str)>,
{
    notes
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(note_id, content)| {
            let visible = visible_text(content);
            SearchResult {
                note_id,
                label: label_of_visible(&visible),
                snippet: snippet_around(&visible, 0),
                match_count: 0,
                matched_text: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> Uuid {
        Uuid::from_bytes([byte; 16])
    }

    fn found(query: &str, notes: &[(Uuid, &str)]) -> Vec<Uuid> {
        search_notes(query, notes.iter().copied())
            .into_iter()
            .map(|result| result.note_id)
            .collect()
    }

    #[test]
    fn folding_removes_case_and_latin_diacritics() {
        assert_eq!(fold("Biópsia").text, "biopsia");
        assert_eq!(fold("CORAÇÃO").text, "coracao");
        assert_eq!(fold("Ünïcödé").text, "unicode");
        assert_eq!(fold("fígado").text, "figado");
        // Text that arrives decomposed folds the same way as precomposed text.
        assert_eq!(fold("bio\u{0301}psia").text, fold("biópsia").text);
    }

    #[test]
    fn folding_leaves_alone_what_it_does_not_know() {
        // No stemming, no transliteration, no guessing.
        assert_eq!(fold("日本語").text, "日本語");
        assert_eq!(fold("Привет").text, "привет");
        assert_eq!(fold("🎉 festa").text, "🎉 festa");
    }

    #[test]
    fn a_query_is_literal_text_and_never_a_pattern() {
        let notes = [
            (id(1), "um texto qualquer"),
            (id(2), "isto contém .* mesmo assim"),
        ];
        // `.*` matches only a note that literally contains `.*`.
        assert_eq!(found(".*", &notes), vec![id(2)]);
        assert_eq!(found("[a-z]", &notes), Vec::<Uuid>::new());
        assert_eq!(found("(um|isto)", &notes), Vec::<Uuid>::new());
    }

    #[test]
    fn search_is_case_and_accent_insensitive() {
        let notes = [
            (id(1), "Biópsia hepática\n\nA biópsia transjugular..."),
            (id(2), "Anatomia do fígado"),
            (id(3), "BIÓPSIA renal"),
        ];
        assert_eq!(found("biopsia", &notes), vec![id(1), id(3)]);
        assert_eq!(found("BIOPSIA", &notes), vec![id(1), id(3)]);
        assert_eq!(found("biópsia", &notes), vec![id(1), id(3)]);
        assert_eq!(found("figado", &notes), vec![id(2)]);
        assert_eq!(found("coracao", &[(id(9), "o Coração bate")]), vec![id(9)]);
    }

    #[test]
    fn a_query_matches_at_the_start_the_middle_and_the_end() {
        let notes = [(id(1), "alpha beta gamma")];
        assert_eq!(found("alpha", &notes), vec![id(1)]);
        assert_eq!(found("beta", &notes), vec![id(1)]);
        assert_eq!(found("gamma", &notes), vec![id(1)]);
        assert_eq!(found("ph", &notes), vec![id(1)]);
    }

    #[test]
    fn nothing_matches_nothing() {
        assert!(found("ausente", &[(id(1), "presente")]).is_empty());
        assert!(found("qualquer", &[]).is_empty());
        assert!(found("qualquer", &[(id(1), "")]).is_empty());
    }

    #[test]
    fn an_empty_query_finds_nothing_rather_than_everything() {
        let notes = [(id(1), "alguma coisa")];
        assert!(found("", &notes).is_empty());
        assert!(found("   ", &notes).is_empty());
        assert!(prepare_query("").is_none());
    }

    #[test]
    fn a_query_past_the_ceiling_is_refused() {
        let long = "a".repeat(MAX_QUERY_CHARS + 1);
        assert!(prepare_query(&long).is_none());
        assert!(found(&long, &[(id(1), "aaaa")]).is_empty());
        // ...and one right at the ceiling still works.
        assert!(prepare_query(&"a".repeat(MAX_QUERY_CHARS)).is_some());
    }

    #[test]
    fn every_occurrence_in_a_note_is_counted_once_and_listed_once() {
        let notes = [(id(1), "fígado, rim, fígado, pulmão, fígado")];
        let results = search_notes("figado", notes.iter().copied());
        assert_eq!(results.len(), 1, "one note is one result");
        assert_eq!(results[0].match_count, 3);
    }

    #[test]
    fn the_result_list_is_capped() {
        let contents: Vec<(Uuid, String)> = (0..(MAX_RESULTS + 40))
            .map(|index| (id(index as u8), format!("nota {index} com agulha")))
            .collect();
        let borrowed: Vec<(Uuid, &str)> = contents
            .iter()
            .map(|(id, text)| (*id, text.as_str()))
            .collect();
        assert_eq!(
            search_notes("agulha", borrowed.iter().copied()).len(),
            MAX_RESULTS
        );
    }

    #[test]
    fn results_keep_the_order_they_were_given() {
        let notes = [
            (id(3), "terceira agulha"),
            (id(1), "primeira agulha"),
            (id(2), "segunda agulha"),
        ];
        assert_eq!(found("agulha", &notes), vec![id(3), id(1), id(2)]);
    }

    #[test]
    fn the_label_is_the_first_line_that_says_something() {
        assert_eq!(label_for("# Biópsia hepática\n\ncorpo"), "Biópsia hepática");
        assert_eq!(
            label_for("\n\n\nprimeira linha real"),
            "primeira linha real"
        );
        assert_eq!(label_for("- [ ] comprar pão"), "comprar pão");
        assert_eq!(label_for("- item de lista"), "item de lista");
        assert_eq!(label_for("> uma citação"), "uma citação");
        assert_eq!(label_for("#### Nível quatro"), "Nível quatro");
        assert_eq!(label_for(""), EMPTY_LABEL);
        assert_eq!(label_for("\n \n"), EMPTY_LABEL);
        // A hash that is part of a word is part of the word.
        assert_eq!(label_for("#urgente hoje"), "#urgente hoje");
    }

    #[test]
    fn a_very_long_label_and_snippet_are_cut_on_a_character_boundary() {
        let long = "á".repeat(4000);

        let label = label_for(&long);
        assert!(label.chars().count() <= MAX_LABEL_CHARS + 1);
        assert!(label.ends_with('…'));

        // Folding is what it says it is: a run of `á` is a run of `a`, so the
        // query finds it. Cutting on a character boundary is the point here.
        let results = search_notes("aaa", [(id(1), long.as_str())]);
        assert_eq!(results.len(), 1);
        assert!(results[0].snippet.chars().count() <= MAX_SNIPPET_CHARS + 2);
        assert!(results[0].snippet.chars().all(|c| c == 'á' || c == '…'));
    }

    #[test]
    fn a_match_longer_than_the_snippet_is_cut_rather_than_overflowing_it() {
        let needle = "z".repeat(MAX_SNIPPET_CHARS * 3);
        let content = format!("antes {needle} depois");
        let results = search_notes(&"z".repeat(MAX_QUERY_CHARS), [(id(1), content.as_str())]);
        assert_eq!(results.len(), 1);
        assert!(results[0].snippet.chars().count() <= MAX_SNIPPET_CHARS + 2);
    }

    #[test]
    fn the_result_carries_the_spelling_that_actually_matched() {
        // Folding is how the note was found; this is how the editor will find
        // it again under a rule that does not fold.
        let results = search_notes("biopsia", [(id(1), "A Biópsia hepática foi feita")]);
        assert_eq!(results[0].matched_text, "Biópsia");

        let results = search_notes("CORACAO", [(id(1), "o coração bate")]);
        assert_eq!(results[0].matched_text, "coração");

        // Nothing matched means nothing to carry.
        assert!(recent_notes([(id(1), "uma nota")])[0]
            .matched_text
            .is_empty());
    }

    #[test]
    fn the_snippet_shows_the_text_as_it_was_written() {
        let notes = [(
            id(1),
            "Notas de estudo\n\na biópsia transjugular é utilizada quando a via percutânea...",
        )];
        let results = search_notes("transjugular", notes.iter().copied());
        assert_eq!(results.len(), 1);
        // Folded to find, original to show: the accents are still there.
        assert!(results[0].snippet.contains("biópsia transjugular"));
        assert!(results[0].snippet.contains("percutânea"));
        // And the snippet is one line.
        assert!(!results[0].snippet.contains('\n'));
    }

    #[test]
    fn an_offset_found_in_folded_text_still_points_at_the_right_source_bytes() {
        // Every fold that changes length is a chance to point at the wrong
        // byte: `Ç` is two bytes and folds to one, a combining mark folds to
        // none, and `ß` lowercases to two characters.
        let content = "ÀÉÎÕÜ ÇÃO alvo ß\u{0301}fim";
        let results = search_notes("alvo", [(id(1), content)]);
        assert_eq!(results.len(), 1);
        assert!(results[0].snippet.contains("alvo"));
        assert!(results[0].snippet.contains("ÇÃO"));
    }

    #[test]
    fn an_empty_query_lists_the_notes_instead() {
        let notes = [
            (id(1), "# Compras\n\nleite"),
            (id(2), ""),
            (id(3), "Reunião de sexta"),
        ];
        let listed = recent_notes(notes.iter().copied());
        assert_eq!(
            listed.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["Compras", EMPTY_LABEL, "Reunião de sexta"]
        );
        assert!(listed.iter().all(|result| result.match_count == 0));
    }

    #[test]
    fn unicode_and_emoji_are_searchable_as_themselves() {
        let notes = [(id(1), "festa 🎉 amanhã"), (id(2), "日本語のノート")];
        assert_eq!(found("🎉", &notes), vec![id(1)]);
        assert_eq!(found("日本語", &notes), vec![id(2)]);
        assert_eq!(found("amanha", &notes), vec![id(1)]);
    }

    /// A note using every mark Note-it can write, as its own serializer
    /// spells them. The stored text is the source of truth; none of it is
    /// something a reader typed except the words.
    const FORMATTED: &str = concat!(
        "# <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">",
        "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">teste de verdade</span>",
        "</mark>\n",
        "\n",
        "**OBSERVAÇÃO:** um *itálico*, um ~~riscado~~ e um <u>sublinhado</u>.\n",
        "\n",
        "<span data-note-it-font-size=\"22\" style=\"font-size:22px\">texto grande</span>\n",
        "\n",
        "<!-- esse é um comentário de teste -->\n",
        "\n",
        "> [!WARNING]\n",
        "> Cuidado com a agulha\n",
        "\n",
        "- [x] comprar pão <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->\n",
    );

    /// Every spelling of Note-it's storage that could reach a reader's eye.
    const STORAGE_SPELLINGS: &[&str] = &[
        "data-note-it-color",
        "data-note-it-highlight",
        "data-note-it-font-size",
        "note-it:completed_at",
        "<span",
        "</span>",
        "<mark",
        "</mark>",
        "<u>",
        "<!--",
        "-->",
        "background-color",
        "font-size:22px",
        "[!WARNING]",
    ];

    #[test]
    fn a_label_shows_the_words_and_never_the_storage_around_them() {
        // 3.9UX.R.1. The reported leak: a coloured phrase was named after the
        // span Note-it wrapped it in.
        assert_eq!(
            label_for(
                "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">teste de verdade</span>"
            ),
            "teste de verdade"
        );
        assert_eq!(
            label_for(
                concat!(
                    "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">",
                    "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">teste de verdade</span>",
                    "</mark>",
                )
            ),
            "teste de verdade"
        );
        assert_eq!(label_for("**OBSERVAÇÃO:** algo"), "OBSERVAÇÃO: algo");
        assert_eq!(label_for("*itálico*"), "itálico");
        assert_eq!(label_for("~~riscado~~"), "riscado");
        assert_eq!(label_for("<u>sublinhado</u>"), "sublinhado");
        assert_eq!(
            label_for("<span data-note-it-font-size=\"22\" style=\"font-size:22px\">grande</span>"),
            "grande"
        );
        assert_eq!(
            label_for("<!-- esse é um comentário de teste -->"),
            "esse é um comentário de teste"
        );
        assert_eq!(label_for("> [!WARNING]\n> Cuidado"), "Cuidado");
        assert_eq!(label_for(FORMATTED), "teste de verdade");
    }

    #[test]
    fn no_result_carries_a_shred_of_the_storage_it_came_from() {
        let notes = [(id(1), FORMATTED)];

        let mut surfaces = vec![
            label_for(FORMATTED),
            opening_of(FORMATTED),
            recent_notes(notes.iter().copied())[0].label.clone(),
            recent_notes(notes.iter().copied())[0].snippet.clone(),
        ];
        for query in ["verdade", "observação", "agulha", "comentário", "pão"] {
            let results = search_notes(query, notes.iter().copied());
            assert_eq!(results.len(), 1, "{query:?} has to find the note");
            surfaces.push(results[0].label.clone());
            surfaces.push(results[0].snippet.clone());
            surfaces.push(results[0].matched_text.clone());
        }

        for surface in &surfaces {
            for spelling in STORAGE_SPELLINGS {
                assert!(
                    !surface.contains(spelling),
                    "{spelling:?} reached a reader in {surface:?}",
                );
            }
        }
    }

    #[test]
    fn the_names_note_it_invented_are_not_words_a_query_can_find() {
        // Serialization is not text: a note is found by what it says, and it
        // says nothing about the attributes it happens to be stored with.
        let notes = [(id(1), FORMATTED)];
        for query in STORAGE_SPELLINGS {
            assert!(
                found(query, &notes).is_empty(),
                "{query:?} matched the storage rather than the note",
            );
        }
        // ...and the words in the same note are all still found.
        for query in ["teste de verdade", "OBSERVAÇÃO", "sublinhado", "grande"] {
            assert_eq!(found(query, &notes), vec![id(1)], "{query:?} must be found");
        }
    }

    #[test]
    fn a_reader_who_really_wrote_the_attribute_still_finds_it() {
        // The other half of the contract. A `<` somebody types is stored as
        // `&lt;`, so text about Note-it's own markup is text like any other.
        let typed = "eu escrevi &lt;span data-note-it-color=\"#fff\"&gt; num texto";
        assert_eq!(
            label_for(typed),
            "eu escrevi <span data-note-it-color=\"#fff\"> num texto"
        );
        assert_eq!(found("data-note-it-color", &[(id(1), typed)]), vec![id(1)]);
        // The same word inside a code span is source somebody typed too.
        let quoted = "use `data-note-it-color` no CSS";
        assert_eq!(found("data-note-it-color", &[(id(2), quoted)]), vec![id(2)]);
    }

    #[test]
    fn the_matched_text_is_a_phrase_the_editor_can_find_again() {
        // What travels back is what the note *shows*, because that is what the
        // editor's own find searches. A slice of a `<span>` could never be
        // revealed in the document.
        let content =
            "<mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">Biópsia hepática</mark>";
        let results = search_notes("biopsia hepatica", [(id(1), content)]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "Biópsia hepática");
    }

    #[test]
    fn folding_still_reaches_across_the_marks_a_phrase_was_split_by() {
        // Half a phrase in one colour and half in another is one phrase to the
        // reader, so it is one phrase to search.
        let content = concat!(
            "<span data-note-it-color=\"#64748B\" style=\"color:#64748B\">Biópsia</span>",
            " <mark data-note-it-highlight=\"#FDE68A\" style=\"background-color:#FDE68A\">hepática</mark>",
        );
        let results = search_notes("biopsia hepatica", [(id(1), content)]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "Biópsia hepática");
        assert_eq!(results[0].label, "Biópsia hepática");
    }

    #[test]
    fn presentation_keeps_accents_emoji_and_other_scripts_exactly() {
        let content = concat!(
            "# 🎉 <span data-note-it-color=\"#64748B\" style=\"color:#64748B\">ação</span> ",
            "日本語 — coração\n",
        );
        assert_eq!(label_for(content), "🎉 ação 日本語 — coração");

        let results = search_notes("coracao", [(id(1), content)]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matched_text, "coração");
        assert!(results[0].snippet.contains('🎉'));
        assert!(results[0].snippet.contains("日本語"));
        assert_eq!(found("🎉", &[(id(1), content)]), vec![id(1)]);
    }

    #[test]
    fn every_heading_level_is_named_after_its_words() {
        for level in 1..=6 {
            let content = format!("{} Biópsia hepática\n\ncorpo", "#".repeat(level));
            assert_eq!(label_for(&content), "Biópsia hepática");
        }
    }

    #[test]
    fn a_listing_shows_the_same_clean_text_a_search_does() {
        let notes = [
            (id(1), FORMATTED),
            (id(2), "- [ ] <span data-note-it-color=\"#64748B\" style=\"color:#64748B\">ligar para Ana</span>"),
            (id(3), ""),
        ];
        let listed = recent_notes(notes.iter().copied());
        assert_eq!(
            listed.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["teste de verdade", "ligar para Ana", EMPTY_LABEL]
        );
        // A listing is a listing: nothing matched, so nothing is carried.
        assert!(listed.iter().all(|result| result.match_count == 0));
        assert!(listed.iter().all(|result| result.matched_text.is_empty()));
    }

    #[test]
    fn projecting_for_presentation_never_touches_what_was_given() {
        // Presentation reads. It has no opinion about the file, and nothing
        // here could move `updated_at` because nothing here is a note.
        let content = FORMATTED.to_string();
        let before = content.clone();

        let _ = label_for(&content);
        let _ = opening_of(&content);
        let _ = search_notes("verdade", [(id(1), content.as_str())]);
        let _ = recent_notes([(id(1), content.as_str())]);

        assert_eq!(content, before);
    }

    #[test]
    fn a_very_long_note_is_searched_without_trouble() {
        let big = format!("{}agulha{}", "x".repeat(400_000), "y".repeat(400_000));
        let results = search_notes("agulha", [(id(1), big.as_str())]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_count, 1);
        assert!(results[0].snippet.chars().count() <= MAX_SNIPPET_CHARS + 2);
    }
}
