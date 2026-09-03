//! Measures the Context Engine against synthetic stores.
//!
//! An example rather than a test: it exists to produce numbers for a phase
//! report, not to pass or fail. Build it in release and run it — a debug
//! measurement of a scan would say more about `opt-level = 0` than about the
//! engine.
//!
//! Every store it touches is a fresh temporary directory. It never opens the
//! store on this machine.

use noteit_core::context::{retrieve, ContextRequest, MAX_CANDIDATES};
use noteit_core::filter::NoteFilter;
use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths};
use std::time::{Duration, Instant};

/// Roughly what a sticky note holds: a line to name it, a couple of
/// paragraphs, a tag, a property and a task. Averages about 700 bytes.
fn body(index: usize, matches: bool) -> String {
    let subject = if matches { "arritmia" } else { "gastrite" };
    format!(
        "Nota {index} sobre {subject}\n\n\
         Anotação de estudo com acentuação — revisão, avaliação e observação. \
         O paciente relatou episódios recorrentes durante a semana, com melhora \
         após ajuste da conduta. Ver referências ao final.\n\n\
         Segundo parágrafo com mais texto para dar peso à varredura, porque uma \
         nota de uma linha mediria o custo de abrir arquivos e não o de lê-los.\n\n\
         - [ ] revisar {subject} da nota {index}\n\
         - [x] arquivar material antigo\n"
    )
}

fn build(size: usize) -> (tempfile::TempDir, NoteItCore) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );
    let core = NoteItCore::from_storage(StorageManager::from_paths(paths).expect("open"));
    core.storage().ensure_directories().expect("dirs");

    for index in 0..size {
        // One note in ten mentions the needle, so a "few matches" query is a
        // real search and not a walk that finds nothing.
        let matches = index % 10 == 0;
        let mut document = NoteDocument::new_empty();
        document.content = body(index, matches);
        document.user_metadata = NoteMetadata::try_new(
            vec![if matches { "Cardiologia" } else { "Gastro" }.to_string()],
            vec![NoteProperty {
                key: "bloco".to_string(),
                value: format!("{}", index % 7),
            }],
        )
        .expect("metadata");
        core.storage().save_note_atomic(&document).expect("seed");
    }
    (tmp, core)
}

fn measure(
    core: &NoteItCore,
    request: &ContextRequest,
    runs: usize,
) -> (Duration, Duration, Duration) {
    // One warm-up, discarded: the first pass pays for the page cache.
    let _ = retrieve(core, request).expect("warm up");
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        let answer = retrieve(core, request).expect("measure");
        samples.push(started.elapsed());
        std::hint::black_box(answer);
    }
    samples.sort();
    (
        samples[samples.len() / 2],
        samples[0],
        samples[samples.len() - 1],
    )
}

fn main() {
    let runs: usize = std::env::args()
        .nth(1)
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(9);

    println!("notas,consulta,runs,mediana_ms,min_ms,max_ms,candidatos,omitidos");
    for size in [100usize, 1_000, 10_000] {
        let (_tmp, core) = build(size);

        let cases: Vec<(&str, ContextRequest)> = vec![
            (
                "texto poucos matches",
                ContextRequest {
                    limit: Some(MAX_CANDIDATES),
                    ..ContextRequest::with_query("arritmia")
                },
            ),
            (
                "texto muitos matches",
                ContextRequest {
                    limit: Some(MAX_CANDIDATES),
                    ..ContextRequest::with_query("revisão")
                },
            ),
            (
                "tag + propriedade",
                ContextRequest {
                    filter: NoteFilter::new(
                        vec!["cardiologia".into()],
                        vec![("bloco".into(), "3".into())],
                    ),
                    limit: Some(MAX_CANDIDATES),
                    ..Default::default()
                },
            ),
            (
                "recencia (sem sinal)",
                ContextRequest {
                    limit: Some(MAX_CANDIDATES),
                    ..Default::default()
                },
            ),
            (
                "texto + tarefas",
                ContextRequest {
                    include_tasks: true,
                    limit: Some(MAX_CANDIDATES),
                    ..ContextRequest::with_query("arritmia")
                },
            ),
        ];

        for (name, request) in cases {
            let (median, min, max) = measure(&core, &request, runs);
            let answer = retrieve(&core, &request).expect("answer");
            println!(
                "{size},{name},{runs},{:.1},{:.1},{:.1},{},{}",
                median.as_secs_f64() * 1000.0,
                min.as_secs_f64() * 1000.0,
                max.as_secs_f64() * 1000.0,
                answer.candidates.len(),
                answer.omitted_count,
            );
        }
    }
}
