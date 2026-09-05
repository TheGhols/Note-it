//! Term-level retrieval: the words of a query, and BM25 over them.
//!
//! Until 4.3B the Context Engine could only answer one lexical question — does
//! the whole query occur in this note, spelled as one run of characters? That
//! is a good answer when it is available and no answer at all the moment
//! somebody types the words in a different order, or types three of them and
//! the note uses four. 4.3A measured the gap: R@1 0.333 on the corpus, and
//! two thirds of the queries returning nothing.
//!
//! This module is the smaller question — do the *terms* occur — scored so that
//! "occurs in a short note about nothing else" outranks "occurs once in an
//! encyclopaedia". Nothing here reads a store, holds a note or knows what a
//! candidate is: it takes folded text and gives back numbers, which is what
//! makes it testable against arithmetic instead of against a fixture.
//!
//! ## The normalisation is the product's, not this module's
//!
//! Terms are the maximal runs of `[0-9a-z]` in text already folded by
//! [`crate::search::fold`] — the fold Note-it has used for global search since
//! long before retrieval had a phase of its own. There is deliberately no
//! second fold here: two definitions of "the same word" is exactly the defect
//! that a single authority exists to prevent, and the existing one already
//! handles case and the Latin diacritics this application's readers type.
//!
//! No stemming, no lemmatisation, no stop words, no synonyms, no per-language
//! table. Each of those is a claim about language that would need its own
//! evidence, and none of them was measured.

use crate::search::Folded;
use std::collections::BTreeMap;

/// BM25's term-frequency saturation.
///
/// Frozen at the canonical value by 4.3A.R1, **before** the corpus was measured
/// rather than after. Tuning a parameter on a set and then publishing that
/// set's score as validation measures the tuning; with thirty-two queries the
/// adjustment would fit entirely inside the noise. Moving it needs a separate
/// tuning set, a separate evaluation set and an explicit decision, in that
/// order.
pub const BM25_K1: f64 = 1.2;

/// BM25's length normalisation. Frozen for the same reason as [`BM25_K1`].
pub const BM25_B: f64 = 0.75;

/// One term of already-folded text, and where it begins in it.
///
/// The offset is what turns a scored match back into something a person can
/// see: it is the way to the occurrence in the note's own spelling, through
/// [`crate::search::source_offset_of`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term<'a> {
    pub text: &'a str,
    pub at: usize,
}

/// Every term of folded text, in the order it appears.
///
/// A term is a maximal run of ASCII digits and lowercase letters. Everything
/// else separates: punctuation, whitespace, and every character the fold left
/// alone because it is not Latin — a CJK sentence, an emoji or a Greek word
/// produces no terms at all, which is honest. Pretending a script this fold
/// does not know is a single term would be inventing a tokenisation for it.
pub fn terms(folded: &str) -> Vec<Term<'_>> {
    let bytes = folded.as_bytes();
    let mut found = Vec::new();
    let mut start = None;

    for index in 0..=bytes.len() {
        let is_term_byte = index < bytes.len() && is_term_byte(bytes[index]);
        match (start, is_term_byte) {
            (None, true) => start = Some(index),
            (Some(from), false) => {
                found.push(Term {
                    text: &folded[from..index],
                    at: from,
                });
                start = None;
            }
            _ => {}
        }
    }

    found
}

/// The alphabet, in one place. ASCII only, which is what the fold produces for
/// every letter it knows.
fn is_term_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || byte.is_ascii_lowercase()
}

/// The distinct terms of one query, in the order they were first typed.
///
/// **Distinct**, and that is a decision. `"sono sono turno"` must not weigh
/// `sono` twice because somebody's finger slipped: a repeated word in a query
/// says nothing about the notes, and letting it double a term's contribution
/// would make the ranking depend on a typo. The query's term frequency is
/// therefore not a weight anywhere in this module.
///
/// **In first-typed order**, because that order is what picks the occurrence a
/// candidate shows as evidence. Sorting the terms would make the snippet depend
/// on the alphabet rather than on what was asked.
#[derive(Debug, Clone, Default)]
pub struct QueryTerms {
    terms: Vec<String>,
    position: BTreeMap<String, usize>,
}

impl QueryTerms {
    /// The terms of a folded query.
    pub fn of(query: &Folded) -> Self {
        let mut terms: Vec<String> = Vec::new();
        let mut position = BTreeMap::new();
        for term in terms_owned(&query.text) {
            if !position.contains_key(&term) {
                position.insert(term.clone(), terms.len());
                terms.push(term);
            }
        }
        Self { terms, position }
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// The terms, in the order they were first typed.
    pub fn as_slice(&self) -> &[String] {
        &self.terms
    }

    /// Which of these terms a piece of folded text contains, and how often.
    ///
    /// One pass, and it keeps only what BM25 needs: the count of each *query*
    /// term and the document's total length. The note's own vocabulary is never
    /// materialised — a store of five thousand notes would otherwise hold five
    /// thousand token tables at once to compute a number that needs two.
    pub fn count_in(&self, folded: &str) -> DocumentTerms {
        let mut frequencies = vec![0u32; self.terms.len()];
        let mut length = 0u32;
        for term in terms(folded) {
            length = length.saturating_add(1);
            if let Some(index) = self.position.get(term.text) {
                frequencies[*index] = frequencies[*index].saturating_add(1);
            }
        }
        DocumentTerms {
            frequencies,
            length,
        }
    }
}

fn terms_owned(folded: &str) -> Vec<String> {
    terms(folded)
        .into_iter()
        .map(|term| term.text.to_string())
        .collect()
}

/// What one document contributes to BM25, for one query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentTerms {
    /// `tf(t, d)` for each query term, in [`QueryTerms`]' order.
    frequencies: Vec<u32>,
    /// `dl`: how many terms the document has, all of them, not just the
    /// query's.
    length: u32,
}

impl DocumentTerms {
    /// Whether any term of the query occurs here at all.
    pub fn matched(&self) -> bool {
        self.frequencies.iter().any(|count| *count > 0)
    }

    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn frequency(&self, term: usize) -> u32 {
        self.frequencies.get(term).copied().unwrap_or(0)
    }
}

/// What the corpus looks like, accumulated one document at a time.
///
/// The corpus is **the live notes that were read successfully for this one
/// retrieval** — see `docs/semantic-retrieval.md` §13. The trash is not in it,
/// because a note somebody deleted must not shape the ranking of the ones they
/// kept; a note that could not be read is not in it either, because a document
/// nobody could parse has no length and no terms, and counting it as an empty
/// one would quietly shorten every other note's normalisation.
#[derive(Debug, Clone, Default)]
pub struct CorpusStatistics {
    documents: u64,
    total_length: u64,
    /// `df(t)`: how many documents contain each query term at least once.
    document_frequency: Vec<u64>,
}

impl CorpusStatistics {
    pub fn for_query(terms: &QueryTerms) -> Self {
        Self {
            documents: 0,
            total_length: 0,
            document_frequency: vec![0; terms.len()],
        }
    }

    /// Folds one readable document into the statistics.
    pub fn observe(&mut self, document: &DocumentTerms) {
        self.documents += 1;
        self.total_length += u64::from(document.length);
        for (index, count) in document.frequencies.iter().enumerate() {
            if *count > 0 {
                if let Some(frequency) = self.document_frequency.get_mut(index) {
                    *frequency += 1;
                }
            }
        }
    }

    pub fn documents(&self) -> u64 {
        self.documents
    }

    /// `avgdl`, or zero when there is nothing to average.
    ///
    /// Zero is a real answer here — a store of thirty empty notes has an
    /// average length of zero — and it is the one value BM25's denominator
    /// cannot take. [`Self::score`] refuses rather than dividing.
    pub fn average_length(&self) -> f64 {
        if self.documents == 0 {
            return 0.0;
        }
        self.total_length as f64 / self.documents as f64
    }

    /// `IDF(t) = ln(1 + (N - df + 0.5) / (df + 0.5))`.
    ///
    /// The `1 +` form, which is what keeps the value positive for a term that
    /// occurs in more than half the corpus. The textbook form without it goes
    /// negative there, and a negative contribution means a note is punished for
    /// containing a word somebody searched for.
    fn inverse_document_frequency(&self, term: usize) -> f64 {
        let frequency = self.document_frequency.get(term).copied().unwrap_or(0) as f64;
        let documents = self.documents as f64;
        (1.0 + (documents - frequency + 0.5) / (frequency + 0.5)).ln()
    }

    /// BM25 for one document against the query these statistics were built for.
    ///
    /// ```text
    /// score(q,d) = Σ IDF(t) · tf(t,d)·(k1+1) / ( tf(t,d) + k1·(1 - b + b·dl/avgdl) )
    /// ```
    ///
    /// Zero when there is no corpus to speak of, and zero rather than `NaN`
    /// when the average length is zero: the channel is empty, which is a
    /// statement, where `0/0` is a bug that ranks.
    pub fn score(&self, document: &DocumentTerms) -> f64 {
        let average = self.average_length();
        if self.documents == 0 || average <= 0.0 {
            return 0.0;
        }

        let normalisation =
            BM25_K1 * (1.0 - BM25_B + BM25_B * f64::from(document.length) / average);
        let mut score = 0.0;
        for (term, count) in document.frequencies.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            let frequency = f64::from(*count);
            score += self.inverse_document_frequency(term) * frequency * (BM25_K1 + 1.0)
                / (frequency + normalisation);
        }
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::fold;

    fn query(text: &str) -> QueryTerms {
        QueryTerms::of(&fold(text))
    }

    fn words(text: &str) -> Vec<String> {
        terms(&fold(text).text)
            .into_iter()
            .map(|term| term.text.to_string())
            .collect()
    }

    #[test]
    fn a_term_is_a_run_of_folded_letters_and_digits() {
        assert_eq!(words("hipertensão arterial"), ["hipertensao", "arterial"]);
        assert_eq!(words("HIPERTENSÃO"), ["hipertensao"]);
        assert_eq!(words("CKD-EPI"), ["ckd", "epi"]);
        assert_eq!(words("covid19, e 2025!"), ["covid19", "e", "2025"]);
    }

    #[test]
    fn punctuation_and_scripts_the_fold_does_not_know_produce_no_terms() {
        assert!(words("!!! ??? ...").is_empty());
        assert!(words("🙂🙂").is_empty());
        assert!(words("心血管").is_empty());
        // Combining marks are dropped by the fold, so a query of nothing but
        // marks has nothing left to tokenise.
        assert!(words("\u{0301}\u{0302}").is_empty());
    }

    #[test]
    fn a_decomposed_letter_tokenises_as_the_precomposed_one() {
        assert_eq!(words("corac\u{0327}ao"), words("coração"));
    }

    #[test]
    fn a_term_knows_where_it_starts() {
        let folded = fold("apneia do sono");
        let found = terms(&folded.text);
        assert_eq!(
            found[0],
            Term {
                text: "apneia",
                at: 0
            }
        );
        assert_eq!(
            found[2],
            Term {
                text: "sono",
                at: 10
            }
        );
    }

    #[test]
    fn a_repeated_query_word_is_one_term_and_keeps_its_first_position() {
        let terms = query("sono sono turno");
        assert_eq!(terms.as_slice(), ["sono", "turno"]);
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn counting_reports_the_documents_whole_length_and_not_only_the_matches() {
        let terms = query("sono");
        let counted = terms.count_in(&fold("o sono do plantonista e o sono do idoso").text);
        assert_eq!(counted.frequency(0), 2);
        assert_eq!(counted.length(), 9);
        assert!(counted.matched());
    }

    #[test]
    fn a_document_with_none_of_the_terms_matched_nothing() {
        let terms = query("metformina");
        let counted = terms.count_in(&fold("insulina e dieta").text);
        assert!(!counted.matched());
        assert_eq!(counted.length(), 3);
    }

    #[test]
    fn an_empty_corpus_scores_nothing_rather_than_dividing_by_it() {
        let terms = query("sono");
        let statistics = CorpusStatistics::for_query(&terms);
        let counted = terms.count_in(&fold("sono").text);
        assert_eq!(statistics.documents(), 0);
        assert_eq!(statistics.average_length(), 0.0);
        assert_eq!(statistics.score(&counted), 0.0);
    }

    #[test]
    fn a_corpus_of_empty_documents_has_no_channel_and_no_nan() {
        let terms = query("sono");
        let mut statistics = CorpusStatistics::for_query(&terms);
        for _ in 0..3 {
            statistics.observe(&terms.count_in(""));
        }
        assert_eq!(statistics.average_length(), 0.0);
        let score = statistics.score(&terms.count_in(""));
        assert!(score.is_finite());
        assert_eq!(score, 0.0);
    }

    /// Three documents, one term, and the arithmetic written out by hand.
    #[test]
    fn the_formula_is_the_published_one() {
        let terms = query("sono");
        let documents = ["sono", "sono sono", "nada disso"];
        let counted: Vec<DocumentTerms> = documents
            .iter()
            .map(|text| terms.count_in(&fold(text).text))
            .collect();
        let mut statistics = CorpusStatistics::for_query(&terms);
        for document in &counted {
            statistics.observe(document);
        }

        // N = 3, df = 2, lengths 1, 2 and 2, so avgdl = 5/3.
        assert_eq!(statistics.documents(), 3);
        let average = 5.0 / 3.0;
        assert!((statistics.average_length() - average).abs() < 1e-12);

        let idf: f64 = (1.0_f64 + (3.0 - 2.0 + 0.5) / (2.0 + 0.5)).ln();
        let by_hand = |frequency: f64, length: f64| {
            idf * frequency * (BM25_K1 + 1.0)
                / (frequency + BM25_K1 * (1.0 - BM25_B + BM25_B * length / average))
        };

        assert!((statistics.score(&counted[0]) - by_hand(1.0, 1.0)).abs() < 1e-12);
        assert!((statistics.score(&counted[1]) - by_hand(2.0, 2.0)).abs() < 1e-12);
        assert_eq!(statistics.score(&counted[2]), 0.0);
    }

    #[test]
    fn a_rare_term_is_worth_more_than_a_common_one() {
        let terms = query("raro comum");
        let mut statistics = CorpusStatistics::for_query(&terms);
        // Ten documents: all carry `comum`, one also carries `raro`.
        let rare = terms.count_in(&fold("raro comum").text);
        let common = terms.count_in(&fold("comum outra").text);
        statistics.observe(&rare);
        for _ in 0..9 {
            statistics.observe(&common);
        }
        assert!(
            statistics.inverse_document_frequency(0) > statistics.inverse_document_frequency(1),
            "a term in one document out of ten must outweigh one in all ten"
        );
        assert!(statistics.score(&rare) > statistics.score(&common));
    }

    #[test]
    fn frequency_helps_but_saturates() {
        let terms = query("sono");
        let mut statistics = CorpusStatistics::for_query(&terms);
        // Every document is exactly thirty terms long, so length normalisation
        // is the same for all of them and only the term frequency can move the
        // score.
        let of = |repeats: usize| {
            let body = format!("{}{}", "sono ".repeat(repeats), "x ".repeat(30 - repeats));
            terms.count_in(&fold(&body).text)
        };
        let counted: Vec<DocumentTerms> = [1, 2, 10, 11].iter().map(|n| of(*n)).collect();
        for document in &counted {
            statistics.observe(document);
        }
        for document in &counted {
            assert_eq!(document.length(), 30);
        }

        let score: Vec<f64> = counted
            .iter()
            .map(|document| statistics.score(document))
            .collect();
        assert!(score[1] > score[0], "more occurrences must score higher");
        assert!(score[2] > score[1]);
        assert!(score[3] > score[2]);
        assert!(
            score[3] - score[2] < score[1] - score[0],
            "and one more occurrence must be worth less the more there already are: \
             that is what k1 is for"
        );
    }

    #[test]
    fn a_short_document_outranks_a_long_one_that_says_the_same_thing() {
        let terms = query("sono");
        let short = terms.count_in(&fold("sono").text);
        let long = terms.count_in(&fold(&format!("sono {}", "palavra ".repeat(200))).text);
        let mut statistics = CorpusStatistics::for_query(&terms);
        statistics.observe(&short);
        statistics.observe(&long);
        assert!(statistics.score(&short) > statistics.score(&long));
    }

    #[test]
    fn two_documents_that_look_the_same_score_the_same() {
        let terms = query("sono turno");
        let left = terms.count_in(&fold("sono no turno").text);
        let right = terms.count_in(&fold("turno de sono").text);
        let mut statistics = CorpusStatistics::for_query(&terms);
        statistics.observe(&left);
        statistics.observe(&right);
        assert_eq!(statistics.score(&left), statistics.score(&right));
    }

    #[test]
    fn a_score_is_always_a_number() {
        let terms = query("sono");
        let mut statistics = CorpusStatistics::for_query(&terms);
        let documents: Vec<DocumentTerms> = [
            "",
            "sono",
            "sono ".repeat(5000).as_str(),
            "palavra ".repeat(5000).as_str(),
        ]
        .iter()
        .map(|text| terms.count_in(&fold(text).text))
        .collect();
        for document in &documents {
            statistics.observe(document);
        }
        for document in &documents {
            assert!(statistics.score(document).is_finite());
            assert!(statistics.score(document) >= 0.0);
        }
    }
}
