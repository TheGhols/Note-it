//! What the local semantic channel costs, in release, measured rather than
//! estimated.
//!
//! Ignored by default. It builds ten thousand notes and reads half a gigabyte
//! of weights, which is minutes rather than seconds and has no business in a
//! pull request's feedback loop. Run it deliberately:
//!
//! ```text
//! cargo test -p noteit-embedding-local --release --test semantic_performance -- --ignored --nocapture
//! ```
//!
//! **Release, and only release.** A debug build of a SHA-256 over 489 MiB
//! measures the absence of optimisation, and quoting that as the cost of
//! loading a model would be quoting the wrong thing.
//!
//! Every store here is synthetic and lives in a temporary directory. No real
//! note is read and none can be: there is no path here that names the store on
//! this machine.
//!
//! ## The budgets, and where they came from
//!
//! `docs/semantic-retrieval.md` §25 sets them, and they were derived from a
//! Python prototype. The one about loading the artifact was derived from an
//! operation that **did not verify it** — §5.1, which makes the verification
//! mandatory, was written later. This test therefore reports the load in two
//! parts, because they answer two different questions and only one of them is
//! what the budget measured.

use noteit_core::chrono::{Duration as ChronoDuration, TimeZone, Utc};
use noteit_core::context::{retrieve_with, ContextRequest, RetrievalMode, MAX_CANDIDATES};
use noteit_core::model::NoteDocument;
use noteit_core::semantic::{
    index_document, EmbeddingProvider, InMemoryIndex, SemanticIndex, SemanticRuntime,
};
use noteit_core::{NoteItCore, StorageManager, StorePaths, Uuid};
use noteit_embedding_local::{artifact_directory, LocalProvider, POTION_MULTILINGUAL_128M};
use std::time::Instant;
use tempfile::tempdir;

/// Resident set size in kibibytes, as the kernel reports it.
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("VmRSS:"))
                .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[(((sorted.len() - 1) as f64) * fraction).round() as usize]
}

/// Paragraph-shaped Portuguese, varied so the tokenizer is not measured on one
/// repeated cache-friendly string.
fn body(index: usize) -> String {
    const SEEDS: [&str; 6] = [
        "O paciente apresenta hipertensão arterial sistêmica, com pressão de 150 por 95 milímetros de mercúrio aferida em consultório.",
        "Depois do plantão noturno tenho dificuldade para pegar no sono; o quarto fica claro e o barulho da rua atrapalha bastante.",
        "Reunião semanal da equipe de produto: revisão do backlog, prioridades do trimestre e alinhamento com o time de engenharia.",
        "Anotação de leitura sobre arquitetura de software, acoplamento, coesão e a diferença entre fronteira de módulo e de processo.",
        "Receita de pão de fermentação natural: 500 g de farinha, 350 g de água, 100 g de levain e 10 g de sal, com dobras a cada meia hora.",
        "Estudo de espanhol: os verbos irregulares no pretérito indefinido e as diferenças de uso entre ser e estar no cotidiano.",
    ];
    format!(
        "{}\n\nRegistro número {index} do caderno, escrito numa quinta-feira qualquer.",
        SEEDS[index % SEEDS.len()]
    )
}

#[test]
#[ignore = "reads half a gigabyte and builds ten thousand notes; run explicitly with --ignored in release"]
fn what_the_local_semantic_channel_costs() {
    let Some(directory) = artifact_directory(&POTION_MULTILINGUAL_128M) else {
        println!("sem diretório XDG de cache; nada a medir");
        return;
    };
    if !directory.join("model.safetensors").is_file() {
        println!(
            "artefato ausente em {}; rode scripts/fetch-embedding-artifact",
            directory.display()
        );
        return;
    }

    println!("\n===================== 4.3C — custo medido, release =====================");
    let baseline = rss_kib();
    println!("RSS antes de qualquer coisa            {baseline:>9} KiB");

    // ---------------------------------------------------------- model load
    let load_started = Instant::now();
    let provider = LocalProvider::load(&directory, &POTION_MULTILINGUAL_128M).expect("load");
    let load = load_started.elapsed();
    let loaded = rss_kib();
    let weights = std::fs::metadata(directory.join("model.safetensors"))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let tokenizer = std::fs::metadata(directory.join("tokenizer.json"))
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    println!(
        "artefato                               {:>9} KiB (pesos) + {} KiB (tokenizer)",
        weights / 1024,
        tokenizer / 1024
    );
    println!(
        "carga do artefato (ler + VERIFICAR + construir) {:>7.0} ms   [orçamento §25: 2000 ms]",
        load.as_secs_f64() * 1000.0
    );
    println!(
        "RSS depois do modelo                   {loaded:>9} KiB  (delta {} KiB)",
        loaded - baseline
    );
    println!(
        "linhas da tabela                       {:>9}",
        provider.table_rows()
    );

    // The verification, timed on its own, because the budget in §25 was
    // measured against a load that had none.
    let hash_started = Instant::now();
    let bytes = std::fs::read(directory.join("model.safetensors")).expect("weights");
    let read = hash_started.elapsed();
    let digest_started = Instant::now();
    let _ = noteit_core::hashing::sha256_hex(&bytes);
    let digest = digest_started.elapsed();
    println!(
        "  dos quais: ler {:>6.0} ms   sha256 {:>6.0} ms  ({:.0} MiB/s)",
        read.as_secs_f64() * 1000.0,
        digest.as_secs_f64() * 1000.0,
        bytes.len() as f64 / 1048576.0 / digest.as_secs_f64()
    );
    drop(bytes);

    let space = EmbeddingProvider::space(&provider);

    // ---------------------------------------------------------- the scales
    // 5 000 notes is two chunks each, so it is the ten *thousand vectors* §25
    // names — the budget is about the size of the index, and a note is not one
    // vector. 10 000 notes is twenty thousand, reported beside it because that
    // is the store shape the number will actually be quoted for.
    for scale in [100usize, 1_000, 5_000, 10_000] {
        println!("\n--- {scale} notas ---");
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        let paths = StorePaths::from_custom_paths(
            root.join("data/note-it/notes"),
            root.join("config/note-it"),
            root.join("state/note-it"),
            root.join("runtime/note-it"),
        );
        let core =
            NoteItCore::from_storage(StorageManager::from_paths(paths).expect("open storage"));
        core.storage().ensure_directories().expect("dirs");
        let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
        let mut documents = Vec::with_capacity(scale);
        for index in 0..scale {
            let mut document = NoteDocument::new_empty();
            document.metadata.id = Uuid::from_u128(0x4e6f_7465_0000_0000u128 + index as u128);
            document.metadata.created_at = Some(base);
            document.metadata.updated_at = Some(base + ChronoDuration::seconds(index as i64));
            document.content = body(index);
            core.storage().save_note_atomic(&document).expect("save");
            documents.push(core.read_note(&document.metadata.id).expect("read back"));
        }

        // Cold index: chunk, embed and insert, from nothing.
        let mut index = InMemoryIndex::new(space.clone());
        let cold_started = Instant::now();
        let mut vectors = 0usize;
        for document in &documents {
            vectors += index_document(document, &provider, &mut index).expect("index");
        }
        let cold = cold_started.elapsed();
        let budget = if scale <= 1_000 { 2_000.0 } else { 20_000.0 };
        let _ = &budget;
        println!(
            "indexação a frio                       {:>9.0} ms   [orçamento §25: {budget:.0} ms]  {vectors} vetores",
            cold.as_secs_f64() * 1000.0
        );
        println!(
            "RSS depois do índice                   {:>9} KiB",
            rss_kib()
        );

        // Hot query, over an index that is already built.
        let mut samples = Vec::new();
        for _ in 0..50 {
            let started = Instant::now();
            let embedded = provider
                .embed_query("o que fazer com pressão alta depois do plantão")
                .expect("embed");
            let hits = index
                .nearest_notes(&embedded, MAX_CANDIDATES)
                .expect("search");
            std::hint::black_box(hits);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "consulta quente sobre {:>6} vetores  {:>9.3} ms p50   {:.3} ms p95   [orçamento §25: 20 ms @ 10 000 vetores]",
            index.vector_count(),
            percentile(&samples, 0.50),
            percentile(&samples, 0.95)
        );

        // One note reindexed. The whole point of the incremental rule.
        let mut single = Vec::new();
        for _ in 0..20 {
            let started = Instant::now();
            index_document(&documents[0], &provider, &mut index).expect("reindex");
            single.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        single.sort_by(f64::total_cmp);
        println!(
            "reindexar UMA nota                     {:>9.3} ms p50   {:.3} ms p95",
            percentile(&single, 0.50),
            percentile(&single, 0.95)
        );

        // The whole retrieval, through the engine, on an index already built.
        let request = ContextRequest {
            query: "pressão alta depois do plantão".to_string(),
            limit: Some(MAX_CANDIDATES),
            ..ContextRequest::default()
        };
        let mut end_to_end = Vec::new();
        for _ in 0..5 {
            let started = Instant::now();
            let runtime = SemanticRuntime::new(&provider, &mut index);
            let answer = retrieve_with(&core, &request, RetrievalMode::Semantic(runtime))
                .expect("retrieval");
            std::hint::black_box(answer);
            end_to_end.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        end_to_end.sort_by(f64::total_cmp);
        println!(
            "recuperação completa (varre o store)   {:>9.0} ms p50",
            percentile(&end_to_end, 0.50)
        );

        // Repetition, to see whether anything grows without bound.
        let before_repeats = rss_kib();
        for _ in 0..200 {
            let embedded = provider
                .embed_query("insônia depois do plantão")
                .expect("embed");
            std::hint::black_box(index.nearest_notes(&embedded, 10).expect("search"));
        }
        let after_repeats = rss_kib();
        println!(
            "RSS após 200 consultas                 {after_repeats:>9} KiB  (delta {} KiB)",
            after_repeats as i64 - before_repeats as i64
        );
        println!(
            "vetores no índice                      {:>9}",
            index.vector_count()
        );
    }

    println!(
        "\nRSS final                              {:>9} KiB",
        rss_kib()
    );
    println!("========================================================================\n");
}
