//! The frozen corpus, run through the real engine with the real model.
//!
//! This is the measurement the feature has to justify itself with, and the one
//! that catches a change to the recipe, the chunker or the ranking by number
//! rather than by impression. It builds the corpus into a throwaway store,
//! indexes it with the shipped provider, and asks all thirty-two questions
//! twice: once with the lexical engine alone, once chained.
//!
//! **The per-query rule is the contract, not the average.** For every question
//! the lexical engine already answers, the chained engine must answer it at the
//! same position or better. An average that improved while one answer fell is a
//! regression, and §13 of `docs/semantic-retrieval.md` says so in those words.
//!
//! Without the artifact the test says so and passes: the factory default never
//! has one, and CI does not download half a gigabyte. Provision it with
//! `scripts/fetch-embedding-artifact`.

use noteit_core::chrono::{DateTime, Duration, TimeZone, Utc};
use noteit_core::context::{
    retrieve, retrieve_with, ContextRequest, Reason, RetrievalMode, MAX_CANDIDATES,
};
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::semantic::{index_document, InMemoryIndex, SemanticIndex, SemanticRuntime};
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use noteit_embedding_local::{artifact_directory, LocalProvider, POTION_MULTILINGUAL_128M};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root is the crate's parent")
        .join("docs")
}

fn provider() -> Option<LocalProvider> {
    let directory = artifact_directory(&POTION_MULTILINGUAL_128M)?;
    match LocalProvider::load(&directory, &POTION_MULTILINGUAL_128M) {
        Ok(provider) => Some(provider),
        Err(error) => {
            println!(
                "artefato local ausente ({error:?}); rode scripts/fetch-embedding-artifact \
                 para medir a qualidade da recuperação encadeada"
            );
            None
        }
    }
}

struct Note {
    id: String,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
    content: String,
}

struct Query {
    id: String,
    text: String,
    relevant: Vec<String>,
}

fn load() -> (Vec<Note>, Vec<Query>) {
    let raw = std::fs::read_to_string(docs_dir().join("retrieval-corpus.json"))
        .expect("the corpus travels with the repository");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("the corpus is JSON");
    let notes = json["notas"]
        .as_array()
        .expect("notas")
        .iter()
        .map(|note| Note {
            id: note["id"].as_str().expect("id").to_string(),
            tags: note["tags"]
                .as_array()
                .map(|tags| {
                    tags.iter()
                        .map(|tag| tag.as_str().expect("tag").to_string())
                        .collect()
                })
                .unwrap_or_default(),
            properties: note["propriedades"]
                .as_array()
                .map(|properties| {
                    properties
                        .iter()
                        .map(|property| {
                            (
                                property["chave"].as_str().expect("chave").to_string(),
                                property["valor"].as_str().expect("valor").to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            content: note["conteudo"].as_str().expect("conteudo").to_string(),
        })
        .collect();
    let queries = json["consultas"]
        .as_array()
        .expect("consultas")
        .iter()
        .map(|query| Query {
            id: query["id"].as_str().expect("id").to_string(),
            text: query["consulta"].as_str().expect("consulta").to_string(),
            relevant: query["relevantes"]
                .as_array()
                .expect("relevantes")
                .iter()
                .map(|id| id.as_str().expect("relevante").to_string())
                .collect(),
        })
        .collect();
    (notes, queries)
}

struct Built {
    _tmp: TempDir,
    core: NoteItCore,
    corpus_id: BTreeMap<Uuid, String>,
    documents: Vec<NoteDocument>,
}

/// The same construction the Core's own corpus suite uses: identifiers derived
/// from position and `updated_at` increasing with it, so no tie is decided by
/// chance and the ruler is the same ruler on every run.
fn build(notes: &[Note]) -> Built {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    let core = NoteItCore::from_storage(StorageManager::from_paths(paths).expect("open storage"));
    core.storage().ensure_directories().expect("ensure dirs");

    let base: DateTime<Utc> = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("a real instant");
    let mut corpus_id = BTreeMap::new();
    let mut documents = Vec::new();
    for (position, note) in notes.iter().enumerate() {
        let id = Uuid::from_u128(0x4e6f_7465_4974_436f_7270_7573_0000_0000u128 + position as u128);
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
        document.metadata.created_at = Some(base);
        document.metadata.updated_at = Some(base + Duration::minutes(position as i64));
        document.content = note.content.clone();
        document.user_metadata = NoteMetadata::try_new(
            note.tags.clone(),
            note.properties
                .iter()
                .map(|(key, value)| NoteProperty {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect::<Vec<_>>(),
        )
        .expect("the corpus carries valid metadata");
        core.storage()
            .save_note_atomic(&document)
            .expect("save corpus note");
        corpus_id.insert(id, note.id.clone());
        // The authoritative read, which is what the engine will compare
        // against. Indexing the in-memory document instead makes every record
        // stale the instant it is written.
        documents.push(core.read_note(&id).expect("read back"));
    }
    Built {
        _tmp: tmp,
        core,
        corpus_id,
        documents,
    }
}

fn request(text: &str) -> ContextRequest {
    ContextRequest {
        query: text.to_string(),
        // The whole ceiling, so a hit that fell to position forty is visible as
        // a hit at forty rather than as a disappearance.
        limit: Some(MAX_CANDIDATES),
        ..ContextRequest::default()
    }
}

fn position_of(
    candidates: &[noteit_core::context::Candidate],
    corpus_id: &BTreeMap<Uuid, String>,
    relevant: &[String],
) -> Option<usize> {
    candidates
        .iter()
        .position(|candidate| {
            corpus_id
                .get(&candidate.note_id)
                .map(|id| relevant.contains(id))
                .unwrap_or(false)
        })
        .map(|index| index + 1)
}

#[test]
fn the_chained_engine_beats_the_lexical_one_and_demotes_nothing() {
    let Some(provider) = provider() else { return };
    let (notes, queries) = load();
    let built = build(&notes);

    let mut index = InMemoryIndex::new(noteit_core::semantic::EmbeddingProvider::space(&provider));
    let mut vectors = 0usize;
    for document in &built.documents {
        vectors += index_document(document, &provider, &mut index).expect("index a corpus note");
    }
    assert_eq!(vectors, index.vector_count());

    let mut hits = [0usize; 3]; // R@1, R@3, R@5
    let mut reciprocal = 0.0f64;
    let mut ground_truth = 0usize;
    let mut noise = 0usize;
    let mut regressions: Vec<String> = Vec::new();
    let mut rows: Vec<String> = Vec::new();

    for query in &queries {
        let lexical = retrieve(&built.core, &request(&query.text)).expect("lexical");
        let before = position_of(&lexical.candidates, &built.corpus_id, &query.relevant);

        let runtime = SemanticRuntime::new(&provider, &mut index);
        let chained = retrieve_with(
            &built.core,
            &request(&query.text),
            RetrievalMode::Semantic(runtime),
        )
        .expect("chained");
        let after = position_of(
            &chained.result.candidates,
            &built.corpus_id,
            &query.relevant,
        );

        if query.relevant.is_empty() {
            // A question with no answer. What matters is how many strangers it
            // brings back, not where a hit landed.
            noise += chained.result.candidates.len();
            rows.push(format!(
                "{:<5} sem-resposta       lexical={:<3} encadeado={:<3} candidatos={}",
                query.id,
                lexical.candidates.len(),
                chained.result.candidates.len(),
                chained.result.candidates.len()
            ));
            continue;
        }

        ground_truth += 1;
        if let Some(rank) = after {
            if rank <= 1 {
                hits[0] += 1;
            }
            if rank <= 3 {
                hits[1] += 1;
            }
            if rank <= 5 {
                hits[2] += 1;
            }
            reciprocal += 1.0 / rank as f64;
        }

        let channel = after
            .and_then(|rank| chained.result.candidates.get(rank - 1))
            .map(|candidate| {
                candidate
                    .reasons
                    .iter()
                    .map(|reason| match reason {
                        Reason::TextMatch => "text",
                        Reason::TermMatch => "term",
                        Reason::SemanticMatch => "semantic",
                        Reason::SharedTag => "tag",
                        Reason::PropertyMatch => "property",
                        Reason::TaskMatch => "task",
                        Reason::Recent => "recent",
                    })
                    .collect::<Vec<_>>()
                    .join("+")
            })
            .unwrap_or_else(|| "-".to_string());

        let verdict = match (before, after) {
            (Some(was), Some(is)) if is > was => "REBAIXADO",
            (Some(_), None) => "PERDIDO",
            (None, Some(_)) => "novo",
            _ => "",
        };
        if !verdict.is_empty() && verdict != "novo" {
            regressions.push(format!("{} {before:?} -> {after:?}", query.id));
        }
        rows.push(format!(
            "{:<5} gt={:<16} 4.3B={:<4} 4.3C={:<4} canal={:<24} {verdict}",
            query.id,
            query.relevant.join(","),
            before.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            after.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            channel
        ));
    }

    let total = ground_truth as f64;
    let (r1, r3, r5, mrr) = (
        hits[0] as f64 / total,
        hits[1] as f64 / total,
        hits[2] as f64 / total,
        reciprocal / total,
    );

    println!("\n# corpus congelado — recuperação encadeada (4.3C)");
    println!("modelo         {}", POTION_MULTILINGUAL_128M.model);
    println!("revisão        {}", POTION_MULTILINGUAL_128M.revision);
    println!("dimensão       {}", POTION_MULTILINGUAL_128M.dimension);
    println!("notas          {}", built.documents.len());
    println!("vetores        {vectors}");
    println!(
        "consultas      {} ({ground_truth} com ground truth)",
        queries.len()
    );
    println!("R@1 {r1:.3}   R@3 {r3:.3}   R@5 {r5:.3}   MRR {mrr:.3}");
    println!("ruído nas consultas sem resposta: {noise} candidatos");
    println!("\nconsulta por consulta:");
    for row in &rows {
        println!("  {row}");
    }
    println!();

    // The structural guarantee first, because it is the one that cannot be
    // traded for a better average.
    assert!(
        regressions.is_empty(),
        "a hit the lexical engine already had was demoted or lost by the semantic step: {regressions:?}"
    );

    // Then the gate the feature has to clear to justify an artifact at all.
    assert!(
        r3 >= 0.900,
        "R@3 encadeado is {r3:.3}, below the 0.900 this feature exists to deliver"
    );
    assert!(r1 >= 0.700, "R@1 fell to {r1:.3}");
    assert!(r5 >= 0.933, "R@5 fell to {r5:.3}");
    assert!(mrr >= 0.800, "MRR fell to {mrr:.3}");
}

#[test]
fn the_semantic_channel_stays_bounded_when_nothing_matched() {
    let Some(provider) = provider() else { return };
    let (notes, queries) = load();
    let built = build(&notes);
    let mut index = InMemoryIndex::new(noteit_core::semantic::EmbeddingProvider::space(&provider));
    for document in &built.documents {
        index_document(document, &provider, &mut index).expect("index");
    }

    for query in queries.iter().filter(|query| query.relevant.is_empty()) {
        let lexical = retrieve(&built.core, &request(&query.text)).expect("lexical");
        let runtime = SemanticRuntime::new(&provider, &mut index);
        let chained = retrieve_with(
            &built.core,
            &request(&query.text),
            RetrievalMode::Semantic(runtime),
        )
        .expect("chained");
        let added = chained
            .result
            .candidates
            .len()
            .saturating_sub(lexical.candidates.len());
        assert!(
            added <= 3,
            "{} gained {added} purely semantic candidates; the ceiling is three so that \
             \"I found nothing in your words\" stays legible",
            query.id
        );
    }
}
