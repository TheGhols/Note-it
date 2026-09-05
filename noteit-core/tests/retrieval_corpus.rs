//! The 4.3A corpus, as a regression ruler for the retrieval engine.
//!
//! `docs/retrieval-corpus.json` is thirty invented notes and thirty-two
//! queries with a ground truth. Nothing in it comes from a real note: it exists
//! so that a change to retrieval is *measured* against the same ruler every
//! time instead of judged by impression, and so that this file can never be a
//! reason to read somebody's notes.
//!
//! What it is for, precisely — and the distinction matters, because it is the
//! difference between measuring and tuning:
//!
//! * it answers **"did this change make something that worked stop working?"**,
//!   query by query. That is the assertion below, and it is the contract:
//!   `docs/semantic-retrieval.md` §13 freezes it as "para **toda** consulta do
//!   corpus em que o motor de hoje devolve um acerto, esse acerto continua na
//!   mesma posição ou mais acima". Never on average — an average lets one query
//!   that got worse hide behind ten that got better;
//! * it does **not** answer "is this engine good". Thirty notes and thirty-two
//!   queries separate architectures and do not separate two similar parameters,
//!   and `k1`/`b` were frozen at 1.2 and 0.75 *before* any of this was measured
//!   precisely so that no number here could pull them.
//!
//! The frozen side of the comparison lives in `docs/retrieval-baseline.json`,
//! recorded against the engine as it stood before BM25. Regenerating it is an
//! explicit act with an environment variable, because a baseline that a failing
//! test can rewrite is not a baseline.

use noteit_core::chrono::{DateTime, TimeZone, Utc};
use noteit_core::context::{retrieve, ContextRequest, Reason, MAX_CANDIDATES};
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

/// The variable that turns the freezing test from a no-op into a write.
const FREEZE: &str = "NOTEIT_FREEZE_RETRIEVAL_BASELINE";

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the workspace root is the crate's parent")
        .join("docs")
}

// ------------------------------------------------------------ the corpus

struct CorpusNote {
    id: String,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
    content: String,
}

struct CorpusQuery {
    id: String,
    category: String,
    text: String,
    relevant: Vec<String>,
}

struct Corpus {
    notes: Vec<CorpusNote>,
    queries: Vec<CorpusQuery>,
}

fn load_corpus() -> Corpus {
    let raw = std::fs::read_to_string(docs_dir().join("retrieval-corpus.json"))
        .expect("the corpus travels with the repository");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("the corpus is JSON");

    let notes = json["notas"]
        .as_array()
        .expect("notas")
        .iter()
        .map(|note| CorpusNote {
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
        .map(|query| CorpusQuery {
            id: query["id"].as_str().expect("id").to_string(),
            category: query["categoria"].as_str().expect("categoria").to_string(),
            text: query["consulta"].as_str().expect("consulta").to_string(),
            relevant: query["relevantes"]
                .as_array()
                .expect("relevantes")
                .iter()
                .map(|id| id.as_str().expect("relevante").to_string())
                .collect(),
        })
        .collect();

    Corpus { notes, queries }
}

// -------------------------------------------------------- the built store

struct BuiltStore {
    _tmp: TempDir,
    core: NoteItCore,
    /// Which corpus identifier each note ended up with, so a rank can be read
    /// back in the corpus's own vocabulary instead of in UUIDs.
    corpus_id: BTreeMap<Uuid, String>,
}

/// Builds the corpus into a throwaway store.
///
/// Two things are chosen rather than left to chance, and both are the
/// difference between a reproducible baseline and a coin toss:
///
/// * **identifiers are derived from position**, because the last rule of the
///   published order is `note_id` — a random UUID would make every tie a
///   different answer on every run;
/// * **`updated_at` is set explicitly and increases with position**, because
///   the corpus says so ("a ordem define a recência") and because `Utc::now()`
///   on thirty notes written in a millisecond is not an order.
fn build(corpus: &Corpus) -> BuiltStore {
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
    for (position, note) in corpus.notes.iter().enumerate() {
        let id = Uuid::from_u128(0x4e6f_7465_4974_436f_7270_7573_0000_0000u128 + position as u128);
        let mut document = NoteDocument::new_empty();
        document.metadata.id = id;
        document.metadata.created_at = Some(base);
        document.metadata.updated_at =
            Some(base + noteit_core::chrono::Duration::minutes(position as i64));
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
    }

    BuiltStore {
        _tmp: tmp,
        core,
        corpus_id,
    }
}

// ------------------------------------------------------------ one reading

/// What one query produced, in the corpus's vocabulary.
struct Observation {
    id: String,
    category: String,
    text: String,
    relevant: Vec<String>,
    /// The corpus identifiers the engine returned, in the order it returned
    /// them.
    returned: Vec<String>,
    /// Why each of them came back, in the same order.
    ///
    /// Kept so that a hit at position three can be explained rather than just
    /// counted: what matters is *which* candidates are ahead of it and on what
    /// grounds, and a rank on its own does not say.
    reasons: Vec<Vec<Reason>>,
}

impl Observation {
    /// The one-based position of the first ground-truth hit, if there is one.
    fn hit(&self) -> Option<usize> {
        self.returned
            .iter()
            .position(|id| self.relevant.contains(id))
            .map(|index| index + 1)
    }
}

fn observe(store: &BuiltStore, corpus: &Corpus) -> Vec<Observation> {
    corpus
        .queries
        .iter()
        .map(|query| {
            let request = ContextRequest {
                query: query.text.clone(),
                // The whole ceiling, so a hit that fell to position forty is
                // visible as a hit at forty rather than as a disappearance.
                limit: Some(MAX_CANDIDATES),
                ..ContextRequest::default()
            };
            let answer = retrieve(&store.core, &request).expect("the corpus store answers");
            Observation {
                id: query.id.clone(),
                category: query.category.clone(),
                text: query.text.clone(),
                relevant: query.relevant.clone(),
                returned: answer
                    .candidates
                    .iter()
                    .map(|candidate| {
                        store
                            .corpus_id
                            .get(&candidate.note_id)
                            .expect("every candidate is a corpus note")
                            .clone()
                    })
                    .collect(),
                reasons: answer
                    .candidates
                    .iter()
                    .map(|candidate| candidate.reasons.clone())
                    .collect(),
            }
        })
        .collect()
}

// -------------------------------------------------------------- metrics

struct Metrics {
    at_1: f64,
    at_3: f64,
    at_5: f64,
    mrr: f64,
    /// How many candidates the queries with no right answer brought back.
    noise: usize,
}

fn measure(observations: &[Observation]) -> Metrics {
    let answerable: Vec<&Observation> = observations
        .iter()
        .filter(|observation| !observation.relevant.is_empty())
        .collect();
    let total = answerable.len() as f64;
    let recall_at = |k: usize| {
        answerable
            .iter()
            .filter(|observation| observation.hit().is_some_and(|rank| rank <= k))
            .count() as f64
            / total
    };
    Metrics {
        at_1: recall_at(1),
        at_3: recall_at(3),
        at_5: recall_at(5),
        mrr: answerable
            .iter()
            .map(|observation| observation.hit().map_or(0.0, |rank| 1.0 / rank as f64))
            .sum::<f64>()
            / total,
        noise: observations
            .iter()
            .filter(|observation| observation.relevant.is_empty())
            .map(|observation| observation.returned.len())
            .sum(),
    }
}

// ------------------------------------------------------- the frozen table

fn baseline_path() -> PathBuf {
    docs_dir().join("retrieval-baseline.json")
}

/// One row of the frozen table: what the pre-BM25 engine did with one query.
struct Frozen {
    candidates: usize,
    hit: Option<usize>,
}

fn read_baseline() -> BTreeMap<String, Frozen> {
    let raw = std::fs::read_to_string(baseline_path()).unwrap_or_else(|error| {
        panic!(
            "the frozen baseline must travel with the repository ({}): {error}. \
             Regenerate it against the pre-BM25 engine with {FREEZE}=1",
            baseline_path().display()
        )
    });
    let json: serde_json::Value = serde_json::from_str(&raw).expect("the baseline is JSON");
    json["consultas"]
        .as_array()
        .expect("consultas")
        .iter()
        .map(|row| {
            (
                row["id"].as_str().expect("id").to_string(),
                Frozen {
                    candidates: row["candidatos"].as_u64().expect("candidatos") as usize,
                    hit: row["posicao_do_acerto"].as_u64().map(|rank| rank as usize),
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------- tests

/// The assertion the phase turns on, query by query.
#[test]
fn no_query_that_worked_is_answered_worse_than_it_was() {
    let corpus = load_corpus();
    let store = build(&corpus);
    let observations = observe(&store, &corpus);
    let baseline = read_baseline();
    let metrics = measure(&observations);

    println!(
        "\n{:<5} {:<18} {:>9} {:>9} {:>10}  text",
        "query", "category", "baseline", "now", "state"
    );
    let mut regressions = Vec::new();
    for observation in &observations {
        let frozen = baseline
            .get(&observation.id)
            .unwrap_or_else(|| panic!("{} is not in the frozen baseline", observation.id));
        let now = observation.hit();
        let state = match (frozen.hit, now) {
            (None, None) => "—",
            (None, Some(_)) => "found",
            (Some(before), Some(after)) if after < before => "improved",
            (Some(before), Some(after)) if after == before => "held",
            (Some(_), _) => "REGRESSED",
        };
        if let Some(before) = frozen.hit {
            match now {
                Some(after) if after <= before => {}
                _ => regressions.push(format!(
                    "{} (\"{}\"): was {before}, now {}",
                    observation.id,
                    observation.text,
                    now.map_or("absent".to_string(), |rank| rank.to_string())
                )),
            }
        }
        println!(
            "{:<5} {:<18} {:>9} {:>9} {:>10}  {}",
            observation.id,
            observation.category,
            frozen.hit.map_or("—".to_string(), |rank| rank.to_string()),
            now.map_or("—".to_string(), |rank| rank.to_string()),
            state,
            observation.text
        );
    }

    println!(
        "\nR@1 {:.3}  R@3 {:.3}  R@5 {:.3}  MRR {:.3}  no-answer candidates {}",
        metrics.at_1, metrics.at_3, metrics.at_5, metrics.mrr, metrics.noise
    );

    // A rank on its own does not say why. For every hit that is not first, this
    // prints what is standing in front of it — which is how "the chaining held
    // a class-1 candidate above a better-scoring one" can be read off the run
    // instead of assumed.
    println!("\nwhat stands in front of a hit that is not first:");
    for observation in &observations {
        let Some(rank) = observation.hit() else {
            continue;
        };
        if rank == 1 {
            continue;
        }
        for position in 0..rank - 1 {
            println!(
                "  {} rank {rank}: {} ahead of it, by {:?}",
                observation.id, observation.returned[position], observation.reasons[position]
            );
        }
    }

    assert!(
        regressions.is_empty(),
        "a hit the engine already found may not fall — query by query, never on average:\n  {}",
        regressions.join("\n  ")
    );
}

/// The queries with no right answer, reported rather than hidden.
///
/// A term occurring in a note does not make the note an answer, so the number
/// below is precision diagnostics and not a target: it is recorded so that a
/// later phase can see whether the engine became noisier, which an R@k on the
/// answerable queries alone would never show.
#[test]
fn the_queries_with_no_answer_are_counted_out_loud() {
    let corpus = load_corpus();
    let store = build(&corpus);
    let observations = observe(&store, &corpus);
    let baseline = read_baseline();

    for observation in observations
        .iter()
        .filter(|observation| observation.relevant.is_empty())
    {
        let frozen = &baseline[&observation.id];
        let by_reason = |reason: Reason| {
            observation
                .reasons
                .iter()
                .filter(|reasons| reasons.contains(&reason))
                .count()
        };
        println!(
            "{} \"{}\": {} candidate(s), was {} — text_match {}, term_match {}, semantic_match {}",
            observation.id,
            observation.text,
            observation.returned.len(),
            frozen.candidates,
            by_reason(Reason::TextMatch),
            by_reason(Reason::TermMatch),
            by_reason(Reason::SemanticMatch),
        );
    }
}

/// Rewriting the frozen table is an explicit act, never a side effect.
#[test]
fn freezing_the_baseline_is_an_explicit_act() {
    if std::env::var(FREEZE).ok().as_deref() != Some("1") {
        println!("{FREEZE} is not 1: the frozen baseline was left alone");
        return;
    }

    let corpus = load_corpus();
    let store = build(&corpus);
    let observations = observe(&store, &corpus);
    let metrics = measure(&observations);

    let rows: Vec<serde_json::Value> = observations
        .iter()
        .map(|observation| {
            serde_json::json!({
                "id": observation.id,
                "categoria": observation.category,
                "consulta": observation.text,
                "relevantes": observation.relevant,
                "candidatos": observation.returned.len(),
                "posicao_do_acerto": observation.hit(),
            })
        })
        .collect();

    let document = serde_json::json!({
        "$schema_version": 1,
        "sobre": "Posição, consulta por consulta, do primeiro acerto do ground truth do corpus \
                  no motor de recuperação do Note-it ANTES do BM25 (Fase 4.3B, commit de \
                  baseline 0222883a4284a254156c2af486066b1dcd893644). Régua de regressão: \
                  nenhuma consulta pode responder pior do que isto. Gerado a partir de \
                  docs/retrieval-corpus.json, que é inteiramente sintético — nenhuma linha \
                  vem de nota real.",
        "regenerar": format!(
            "{FREEZE}=1 cargo test -p noteit-core --test retrieval_corpus -- --exact \
             freezing_the_baseline_is_an_explicit_act"
        ),
        "limite_de_candidatos": MAX_CANDIDATES,
        "metricas": {
            "R@1": format!("{:.3}", metrics.at_1),
            "R@3": format!("{:.3}", metrics.at_3),
            "R@5": format!("{:.3}", metrics.at_5),
            "MRR": format!("{:.3}", metrics.mrr),
            "candidatos_nas_consultas_sem_resposta": metrics.noise,
        },
        "consultas": rows,
    });

    let mut text = serde_json::to_string_pretty(&document).expect("serialise the baseline");
    text.push('\n');
    std::fs::write(baseline_path(), text).expect("write the baseline");
    println!("froze {}", baseline_path().display());
}
