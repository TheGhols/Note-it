//! The remote channel, as the product actually reaches it.
//!
//! Everything here goes through `domain::context` — the same function the
//! `noteit_context` tool calls — on a blocking thread obtained from
//! `off_reactor`. The provider is a real [`RemoteProvider`] speaking the real
//! protocol over a real `AF_UNIX` socket; what is on the far side is a worker
//! written in this file.
//!
//! ## What that does and does not prove, said precisely
//!
//! **Proved here:** the client half, the wire, the batching, the space, the
//! cache, the incremental pass, provider switching, the fallback policies, the
//! `SemanticStatus` the answer carries, and that a question is answered while
//! indexing runs.
//!
//! **Not proved here, and proved elsewhere:** everything on the *other* side of
//! that socket. The HTTP client, TLS, redirects, the proxy policy, the status
//! table, the retry schedule, the vendor adapters and the credential are
//! covered by `noteit-embed`'s own suite, which drives the real worker code
//! against a mock HTTP server on loopback; and the real worker *binary* — its
//! environment, its argv, its socket's mode, its lifecycle and its inability to
//! open a note — is covered by
//! `noteit-embedding-remote/tests/worker_isolation.rs`.
//!
//! The seam between the two is the protocol, and it is the same code on both
//! sides: `noteit-embed-protocol`, whose round trip, limits and refusals have
//! twenty-three tests of their own. Splitting the proof this way is deliberate
//! — a suite that needed a built binary, a mock server and an MCP process at
//! once would fail for three reasons at a time.
//!
//! **No test in this file reaches a network.** The fake worker listens on a
//! Unix socket and returns arithmetic.

mod support;

use noteit_core::model::NoteDocument;
use noteit_core::settings::{
    SemanticFallbackPolicy, SemanticMode, SemanticProvider, SemanticRetrievalConfig,
};
use noteit_core::{NoteItCore, StorePaths, Uuid};
use noteit_embed_protocol::{
    read_frame, write_frame, EmbedRequestV1, EmbedResponseV1, ProviderId, WireError,
};
use noteit_embedding_remote::worker::{WorkerHandle, WorkerPaths};
use noteit_embedding_remote::{cache, space, RemoteConfig};
use noteit_mcp::contract::{ContextInput, SemanticStatusView, Status};
use noteit_mcp::domain::{context, off_reactor, Store};
use noteit_mcp::semantic::{ProviderSource, RemoteSource, SemanticSession};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::Sandbox;

const DIMENSION: usize = 8;
const MODEL: &str = "text-embedding-3-small";

// ------------------------------------------------------------- the worker

/// A worker on a Unix socket that returns arithmetic instead of a vendor.
///
/// It speaks the protocol byte for byte — same `read_frame`, same
/// `write_frame`, same validation — so the client is exercised against the
/// contract rather than against a convenience.
struct FakeWorker {
    socket: PathBuf,
    requests: Arc<AtomicUsize>,
    texts: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    /// How long to hold each request before answering. The controlled delay
    /// §56 asks for, in place of a real provider's latency.
    delay: Duration,
    /// What to answer instead of vectors, when a test wants a failure.
    refuse: Arc<Mutex<Option<WireError>>>,
    /// The value every component of a returned vector is offset by, so two
    /// configurations can be told apart by their numbers.
    flavour: f32,
}

impl FakeWorker {
    fn start(socket: PathBuf, delay: Duration, flavour: f32) -> Arc<Self> {
        if let Some(parent) = socket.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        let _ = std::fs::remove_file(&socket);
        let listener =
            std::os::unix::net::UnixListener::bind(&socket).expect("bind the fake worker");
        let worker = Arc::new(Self {
            socket,
            requests: Arc::new(AtomicUsize::new(0)),
            texts: Arc::new(Mutex::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            delay,
            refuse: Arc::new(Mutex::new(None)),
            flavour,
        });
        let serving = Arc::clone(&worker);
        std::thread::spawn(move || {
            for incoming in listener.incoming() {
                if serving.stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = incoming else { continue };
                let serving = Arc::clone(&serving);
                std::thread::spawn(move || serving.answer(stream));
            }
        });
        worker
    }

    fn answer(&self, mut stream: std::os::unix::net::UnixStream) {
        let Ok(request) = read_frame::<_, EmbedRequestV1>(&mut stream) else {
            let _ = write_frame(&mut stream, &EmbedResponseV1::error(WireError::Protocol));
            return;
        };
        // The same validation the real worker applies on receipt.
        if request.validate().is_err() {
            let _ = write_frame(&mut stream, &EmbedResponseV1::error(WireError::Protocol));
            return;
        }
        self.requests.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut seen) = self.texts.lock() {
            seen.extend(request.texts.iter().cloned());
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        if let Some(error) = *self.refuse.lock().expect("refuse") {
            let _ = write_frame(&mut stream, &EmbedResponseV1::error(error));
            return;
        }
        let vectors: Vec<Vec<f32>> = request.texts.iter().map(|text| self.embed(text)).collect();
        let response = EmbedResponseV1::answer(DIMENSION as u32, vectors)
            .unwrap_or_else(EmbedResponseV1::error);
        let _ = write_frame(&mut stream, &response);
        let _ = stream.shutdown(std::net::Shutdown::Write);
    }

    /// A deterministic bag-of-characters embedding.
    ///
    /// Not a model. It only has to be stable, finite and non-zero, and to put
    /// texts that share words nearer each other than texts that do not — which
    /// is enough for the channel to admit a candidate and therefore for
    /// `semantic_status` to be able to say `succeeded`.
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut values = vec![0.0f32; DIMENSION];
        for (index, byte) in text.bytes().enumerate() {
            values[(byte as usize + index / 32) % DIMENSION] += 1.0;
        }
        for (index, value) in values.iter_mut().enumerate() {
            *value += self.flavour + index as f32 * 0.01 + 0.5;
        }
        values
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    fn reset(&self) {
        self.requests.store(0, Ordering::SeqCst);
        self.texts.lock().expect("texts").clear();
    }

    fn texts(&self) -> Vec<String> {
        self.texts.lock().expect("texts").clone()
    }

    fn refuse_with(&self, error: Option<WireError>) {
        *self.refuse.lock().expect("refuse") = error;
    }

    fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::os::unix::net::UnixStream::connect(&self.socket);
        let _ = std::fs::remove_file(&self.socket);
    }
}

// --------------------------------------------------------------- the world

struct World {
    sandbox: Sandbox,
    worker: Arc<FakeWorker>,
    cache_root: PathBuf,
}

impl World {
    fn new(delay: Duration, flavour: f32) -> Self {
        Self::with_notes(delay, flavour, default_notes())
    }

    fn with_notes(delay: Duration, flavour: f32, notes: Vec<(&str, &str)>) -> Self {
        let sandbox = Sandbox::new();
        let paths = sandbox.store_paths();
        std::fs::create_dir_all(&paths.notes_dir).expect("mkdir");
        std::fs::create_dir_all(&paths.runtime_dir).expect("mkdir");
        std::fs::create_dir_all(&paths.config_dir).expect("mkdir");
        for (title, body) in notes {
            write_note(&paths, title, body);
        }
        let socket = paths
            .runtime_dir
            .join(noteit_embedding_remote::worker::SOCKET_NAME);
        let worker = FakeWorker::start(socket, delay, flavour);
        let cache_root = sandbox.root.join("cache/note-it");
        std::fs::create_dir_all(&cache_root).expect("mkdir");
        Self {
            sandbox,
            worker,
            cache_root,
        }
    }

    fn core(&self) -> NoteItCore {
        self.sandbox.core()
    }

    /// A session pointed at the fake worker and this world's cache.
    fn session(&self, fallback: SemanticFallbackPolicy) -> SemanticSession {
        self.session_for(fallback, SemanticProvider::OpenAi, MODEL, DIMENSION, true)
    }

    fn session_for(
        &self,
        fallback: SemanticFallbackPolicy,
        provider: SemanticProvider,
        model: &str,
        dimension: usize,
        with_cache: bool,
    ) -> SemanticSession {
        let paths = self.sandbox.store_paths();
        let settings = SemanticRetrievalConfig {
            mode: SemanticMode::Semantic,
            provider,
            fallback,
            model: Some(model.to_string()),
            dimension: Some(dimension),
        };
        let source = ProviderSource::Remote(Box::new(RemoteSource {
            config: RemoteConfig {
                provider: ProviderId::parse(provider.as_str()).expect("a remote provider"),
                model: model.to_string(),
                dimension,
                dimension_requested: true,
            },
            worker: Arc::new(WorkerHandle::new(WorkerPaths {
                // Never spawned: the fake worker is already listening at this
                // path, and `ensure` adopts a live one rather than starting a
                // second beside it.
                binary: PathBuf::from("/nonexistent/noteit-embed"),
                socket: paths
                    .runtime_dir
                    .join(noteit_embedding_remote::worker::SOCKET_NAME),
                config_dir: paths.config_dir.clone(),
            })),
            config_dir: paths.config_dir.clone(),
            cache_root: with_cache.then(|| self.cache_root.clone()),
        }));
        SemanticSession::with_source(settings, source)
    }

    fn store(&self, session: SemanticSession) -> Store {
        Store::with_semantic_session(self.sandbox.store_paths(), session)
    }

    fn ask(&self, store: &Store, query: &str) -> (Status, SemanticStatusView, usize) {
        let answer = answer_for(store, query);
        (
            answer.status,
            answer.semantic_status,
            answer.candidates.len(),
        )
    }
}

/// One retrieval, through the same offloaded adapter the tool uses.
fn answer_for(store: &Store, query: &str) -> noteit_mcp::contract::ContextResult {
    let store = store.clone();
    let input = query_for(query);
    runtime().block_on(async move {
        off_reactor(&store, move |off, store| context(off, store, input))
            .await
            .expect("the adapter answered")
    })
}

fn query_for(query: &str) -> ContextInput {
    ContextInput {
        query: query.to_string(),
        tags: Vec::new(),
        properties: Vec::new(),
        include_tasks: false,
        limit: Some(50),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// The runtime the concurrency tests use, and it is deliberately the same one.
///
/// `noteit-mcp` builds Tokio with `rt` and **not** `rt-multi-thread`: the
/// protocol needs exactly one thread and the store work does not run on it.
/// So a test that reached for a multi-threaded runtime would be testing a
/// server this repository does not ship — and it would be the *weaker* test.
/// With one reactor thread, "a question is answered while indexing runs" can
/// only be true if the indexing really left the reactor, which is the property
/// `off_reactor` and the `OffThread` witness exist for.
fn threaded_runtime() -> tokio::runtime::Runtime {
    runtime()
}

impl Drop for World {
    fn drop(&mut self) {
        self.worker.stop();
    }
}

fn default_notes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Pressao", "pressao alta e o sal da comida"),
        ("Sono", "insonia depois do plantao da noite"),
        ("Equipe", "reuniao de equipe sobre orcamento"),
        ("Pao", "pao de farinha e fermentacao lenta"),
    ]
}

fn write_note(paths: &StorePaths, title: &str, body: &str) -> Uuid {
    let mut document = NoteDocument::new_empty();
    document.content = format!("# {title}\n\n{body}\n");
    let id = document.metadata.id;
    let core = NoteItCore::from_storage(
        noteit_core::StorageManager::from_paths(paths.clone()).expect("open the store"),
    );
    core.storage().save_note_atomic(&document).expect("save");
    id
}

fn cache_file_exists(root: &std::path::Path, provider: SemanticProvider, model: &str) -> bool {
    let space = space::space_for(
        ProviderId::parse(provider.as_str()).expect("provider"),
        model,
        DIMENSION,
    );
    cache::cache_file(root, &space).is_file()
}

// ============================================================ the channel

#[test]
fn the_remote_channel_answers_and_says_it_succeeded() {
    // §85: `Succeeded` may only be said when the channel actually ran.
    // Without this assertion every test below would pass on a lexical answer.
    let world = World::new(Duration::ZERO, 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (status, semantic, candidates) = world.ask(&store, "pressao alta");
    assert_eq!(status, Status::Ok);
    assert_eq!(
        semantic,
        SemanticStatusView::Succeeded,
        "the remote path did not run"
    );
    assert!(candidates > 0);
    assert!(
        world.worker.requests() > 0,
        "nothing was sent to the worker, so nothing remote happened"
    );
}

#[test]
fn a_worker_that_refuses_degrades_under_automatic_and_says_so() {
    let world = World::new(Duration::ZERO, 0.0);
    world.worker.refuse_with(Some(WireError::RateLimited));
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (status, semantic, _) = world.ask(&store, "pressao alta");
    assert_eq!(status, Status::Ok, "a lexical answer is still an answer");
    assert_eq!(
        semantic,
        SemanticStatusView::Unavailable,
        "a failure was reported as something other than unavailable"
    );
}

#[test]
fn a_worker_that_refuses_is_a_refusal_under_semantic_required() {
    // §42: masking the failure of somebody who asked for semantics on
    // purpose is lying about what was done.
    let world = World::new(Duration::ZERO, 0.0);
    world.worker.refuse_with(Some(WireError::Authentication));
    let store = world.store(world.session(SemanticFallbackPolicy::SemanticRequired));
    let answer = answer_for(&store, "pressao alta");
    assert_eq!(answer.status, Status::Error);
    assert_eq!(
        answer.semantic_status,
        SemanticStatusView::NotRequested,
        "a refusal must not claim the channel succeeded"
    );
    assert!(answer.candidates.is_empty());
}

#[test]
fn a_missing_worker_degrades_rather_than_failing_the_question() {
    let world = World::new(Duration::ZERO, 0.0);
    world.worker.stop();
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (status, semantic, _) = world.ask(&store, "pressao alta");
    assert_eq!(status, Status::Ok);
    assert_eq!(semantic, SemanticStatusView::Unavailable);
}

#[test]
fn lexical_only_never_touches_the_worker() {
    // §43 and §44: the factory default, and the switch that turns the
    // channel off, must cost nothing at all.
    let world = World::new(Duration::ZERO, 0.0);
    for (mode, fallback) in [
        (SemanticMode::LexicalOnly, SemanticFallbackPolicy::Automatic),
        (SemanticMode::Semantic, SemanticFallbackPolicy::LexicalOnly),
    ] {
        world.worker.reset();
        let paths = world.sandbox.store_paths();
        let settings = SemanticRetrievalConfig {
            mode,
            provider: SemanticProvider::OpenAi,
            fallback,
            model: Some(MODEL.to_string()),
            dimension: Some(DIMENSION),
        };
        let session = SemanticSession::new(settings, &paths);
        let store = world.store(session);
        let (status, semantic, _) = world.ask(&store, "pressao alta");
        assert_eq!(status, Status::Ok);
        assert_eq!(semantic, SemanticStatusView::NotRequested);
        assert_eq!(
            world.worker.requests(),
            0,
            "{mode:?}/{fallback:?} sent something to a worker"
        );
    }
}

#[test]
fn a_request_with_no_question_asks_the_worker_nothing() {
    let world = World::new(Duration::ZERO, 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    // An empty query is a request that carries nothing to embed, which is the
    // `NotRequested` case §12 names and not a failure.
    let answer = answer_for(&store, "");
    assert_eq!(answer.semantic_status, SemanticStatusView::NotRequested);
    assert_eq!(world.worker.requests(), 0);
}

// ============================================================== the cache

#[test]
fn a_second_session_reuses_the_cache_and_sends_nothing() {
    // §81: this is the whole reason the remote cache exists. A second
    // start that re-embedded the store would charge the user twice for the
    // same vectors.
    let world = World::new(Duration::ZERO, 0.0);

    let first = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (_, semantic, _) = world.ask(&first, "pressao alta");
    assert_eq!(semantic, SemanticStatusView::Succeeded);
    let cold = world.worker.requests();
    assert!(cold > 0, "the cold pass sent nothing");
    assert!(
        cache_file_exists(&world.cache_root, SemanticProvider::OpenAi, MODEL),
        "the cold pass wrote no cache"
    );

    // A brand new session over the same store and the same cache: what a
    // second start of the MCP server is.
    world.worker.reset();
    let second = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (_, semantic, candidates) = world.ask(&second, "pressao alta");
    assert_eq!(
        semantic,
        SemanticStatusView::Succeeded,
        "the warm pass did not use the semantic channel"
    );
    assert!(candidates > 0);
    assert_eq!(
        world.worker.requests(),
        1,
        "the warm pass re-embedded documents; only the query itself should have been sent"
    );
}

#[test]
fn editing_one_note_re_embeds_that_note_and_not_the_store() {
    // §82 and §40: an edit costs one note's embedding, never the
    // store's.
    let world = World::new(Duration::ZERO, 0.0);
    let paths = world.sandbox.store_paths();

    let first = world.store(world.session(SemanticFallbackPolicy::Automatic));
    world.ask(&first, "pressao alta");
    let cold = world.worker.requests();
    assert!(cold >= 2, "the cold pass sent {cold} requests");

    // One note changes; the other three do not.
    let core = world.core();
    let ids = core.list_notes().expect("list");
    let target = ids[0];
    let mut document = core.read_note(&target).expect("read");
    document.content = "# Pressao\n\npressao alta e o sal e tambem a insonia\n".to_string();
    core.storage().save_note_atomic(&document).expect("save");
    let _ = &paths;

    world.worker.reset();
    let second = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (_, semantic, _) = world.ask(&second, "pressao alta");
    assert_eq!(semantic, SemanticStatusView::Succeeded);

    let sent = world.worker.texts();
    // One request for the query, and one for the edited note's chunks. The
    // other three notes came out of the cache.
    let document_texts: Vec<&String> = sent
        .iter()
        .filter(|text| text.contains("sal") || text.contains("insonia"))
        .collect();
    assert!(
        !document_texts.is_empty(),
        "the edited note was not re-embedded"
    );
    for untouched in ["reuniao de equipe", "pao de farinha"] {
        assert!(
            !sent.iter().any(|text| text.contains(untouched)),
            "an untouched note was re-sent: {untouched}"
        );
    }
    assert!(
        world.worker.requests() <= 3,
        "an edit cost {} requests; only the query and the edited note should have been sent",
        world.worker.requests()
    );
}

#[test]
fn losing_the_cache_costs_money_and_never_a_note() {
    // §33 and §83.
    let world = World::new(Duration::ZERO, 0.0);
    let first = world.store(world.session(SemanticFallbackPolicy::Automatic));
    world.ask(&first, "pressao alta");

    let before: Vec<String> = world
        .core()
        .list_notes()
        .expect("list")
        .iter()
        .map(|id| world.core().read_note(id).expect("read").content)
        .collect();

    std::fs::remove_dir_all(world.cache_root.join("semantic")).expect("clear the cache");

    world.worker.reset();
    let second = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (_, semantic, _) = world.ask(&second, "pressao alta");
    assert_eq!(semantic, SemanticStatusView::Succeeded);
    assert!(
        world.worker.requests() > 1,
        "the cache was not actually gone"
    );

    let after: Vec<String> = world
        .core()
        .list_notes()
        .expect("list")
        .iter()
        .map(|id| world.core().read_note(id).expect("read").content)
        .collect();
    assert_eq!(before, after, "clearing a cache changed a note");
}

#[test]
fn a_corrupt_cache_is_rebuilt_rather_than_used() {
    let world = World::new(Duration::ZERO, 0.0);
    let first = world.store(world.session(SemanticFallbackPolicy::Automatic));
    world.ask(&first, "pressao alta");

    let space = space::space_for(ProviderId::OpenAi, MODEL, DIMENSION);
    let file = cache::cache_file(&world.cache_root, &space);
    let mut bytes = std::fs::read(&file).expect("read");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    std::fs::write(&file, &bytes).expect("write");

    world.worker.reset();
    let second = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (status, semantic, candidates) = world.ask(&second, "pressao alta");
    assert_eq!(status, Status::Ok);
    assert_eq!(
        semantic,
        SemanticStatusView::Succeeded,
        "a damaged cache stopped the channel instead of being rebuilt"
    );
    assert!(candidates > 0);
    assert!(
        world.worker.requests() > 1,
        "a damaged cache was used rather than rebuilt"
    );
}

#[test]
fn the_cache_never_holds_a_notes_text() {
    let world = World::new(Duration::ZERO, 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    world.ask(&store, "pressao alta");
    let space = space::space_for(ProviderId::OpenAi, MODEL, DIMENSION);
    let bytes = std::fs::read(cache::cache_file(&world.cache_root, &space)).expect("read");
    let text = String::from_utf8_lossy(&bytes);
    for phrase in ["pressao alta", "insonia", "orcamento", "fermentacao"] {
        assert!(!text.contains(phrase), "the cache carries `{phrase}`");
    }
}

// ==================================================== switching providers

#[test]
fn every_switch_rebuilds_rather_than_reusing_an_incompatible_index() {
    // §39 and §31. Each pair is a different `EmbeddingSpaceId`, so no
    // vector from one may be compared with a vector from the other — and the
    // proof is that the second configuration sends documents again rather than
    // finding the first's.
    let world = World::new(Duration::ZERO, 0.0);
    let cases: [(SemanticProvider, &str, usize); 5] = [
        (
            SemanticProvider::OpenAi,
            "text-embedding-3-small",
            DIMENSION,
        ),
        // A different provider.
        (SemanticProvider::Voyage, "voyage-4", DIMENSION),
        // A different model of the same provider.
        (SemanticProvider::Voyage, "voyage-4-lite", DIMENSION),
        // A different dimension of the same model.
        (SemanticProvider::Voyage, "voyage-4-lite", DIMENSION),
        // Back to the first, which by then has a cache of its own.
        (
            SemanticProvider::OpenAi,
            "text-embedding-3-small",
            DIMENSION,
        ),
    ];
    let mut seen_spaces = Vec::new();
    for (index, (provider, model, dimension)) in cases.into_iter().enumerate() {
        world.worker.reset();
        let session = world.session_for(
            SemanticFallbackPolicy::Automatic,
            provider,
            model,
            dimension,
            true,
        );
        let store = world.store(session);
        let (status, semantic, _) = world.ask(&store, "pressao alta");
        assert_eq!(status, Status::Ok, "case {index}");
        assert_eq!(semantic, SemanticStatusView::Succeeded, "case {index}");

        let space = space::space_for(
            ProviderId::parse(provider.as_str()).expect("provider"),
            model,
            dimension,
        );
        let digest = space::space_digest(&space);
        if !seen_spaces.contains(&digest) {
            // A space this store has never had: everything is embedded again.
            assert!(
                world.worker.requests() > 1,
                "case {index} reused an index from another space"
            );
            seen_spaces.push(digest);
        }
    }
    // Four distinct configurations produced four distinct spaces — the
    // repeated one is the fourth case, which names the same three fields.
    assert_eq!(seen_spaces.len(), 3);
}

#[test]
fn a_switch_keeps_the_lexical_answer_working() {
    // §39: whatever happens to the index, the lexical channel is not
    // allowed to get worse.
    let world = World::new(Duration::ZERO, 0.0);
    world.worker.refuse_with(Some(WireError::ModelUnavailable));
    let session = world.session_for(
        SemanticFallbackPolicy::Automatic,
        SemanticProvider::Gemini,
        "gemini-embedding-001",
        DIMENSION,
        true,
    );
    let store = world.store(session);
    let (status, semantic, candidates) = world.ask(&store, "pressao alta");
    assert_eq!(status, Status::Ok);
    assert_eq!(semantic, SemanticStatusView::Unavailable);
    assert!(
        candidates > 0,
        "the lexical channel stopped answering when the remote one failed"
    );
}

// ============================================== the reactor and concurrency

#[test]
fn a_question_is_answered_while_a_remote_index_is_being_built() {
    // §56, with a controlled delay in place of a real provider's latency.
    //
    // The simulated part is named: the worker sleeps 40 ms per request, and
    // with four notes the indexing pass therefore takes at least 160 ms of
    // *provider* time. What is measured is the other thread's question, which
    // must not wait for it.
    let world = World::new(Duration::from_millis(40), 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let runtime = threaded_runtime();

    let (answer, elapsed) = runtime.block_on(async {
        let indexing = {
            let store = store.clone();
            let input = query_for("pressao alta");
            tokio::spawn(async move {
                off_reactor(&store, move |off, store| context(off, store, input))
                    .await
                    .expect("the adapter answered")
            })
        };

        // A trivial call on the same reactor while the indexing pass runs. What
        // it measures is that the pass is *off* the reactor — which is what
        // `off_reactor` and the `OffThread` witness exist to guarantee, and
        // what a server that embedded on the protocol thread would fail.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let started = Instant::now();
        let quick = off_reactor(&store, |_off, _store| 1 + 1)
            .await
            .expect("the adapter answered");
        let elapsed = started.elapsed();
        assert_eq!(quick, 2);
        (indexing.await.expect("join"), elapsed)
    });

    assert!(
        elapsed < Duration::from_millis(120),
        "a trivial off-reactor call waited {elapsed:?} behind a remote indexing pass"
    );
    assert_eq!(answer.status, Status::Ok);
    assert_eq!(
        answer.semantic_status,
        SemanticStatusView::Succeeded,
        "the indexing pass did not actually run the remote path, so this proves nothing"
    );
}

#[test]
fn concurrent_questions_on_an_unindexed_store_build_one_index() {
    // §18 and §55: two questions about an unindexed store must not
    // produce two indexing passes — and in the remote mode that is not a
    // performance point, it is the bill.
    let world = World::new(Duration::from_millis(10), 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let runtime = threaded_runtime();

    let answers = runtime.block_on(async {
        let mut tasks = Vec::new();
        for _ in 0..4 {
            let store = store.clone();
            let input = query_for("pressao alta");
            tasks.push(tokio::spawn(async move {
                off_reactor(&store, move |off, store| context(off, store, input))
                    .await
                    .expect("the adapter answered")
            }));
        }
        let mut answers = Vec::new();
        for task in tasks {
            answers.push(task.await.expect("join"));
        }
        answers
    });

    for answer in &answers {
        assert_eq!(answer.status, Status::Ok);
        assert_eq!(answer.semantic_status, SemanticStatusView::Succeeded);
    }

    // Four notes embedded once, plus one query per caller. A second indexing
    // pass would show up as another four document texts.
    let document_texts = world
        .worker
        .texts()
        .iter()
        .filter(|text| {
            text.contains("sal")
                || text.contains("plantao")
                || text.contains("orcamento")
                || text.contains("farinha")
        })
        .count();
    assert!(
        document_texts <= 4,
        "{document_texts} document texts were embedded; four notes were indexed more than once"
    );
}

#[test]
fn a_worker_that_dies_mid_pass_degrades_rather_than_corrupting_the_index() {
    let world = World::new(Duration::ZERO, 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    // A first pass that works, and a cache written from it.
    let (_, semantic, _) = world.ask(&store, "pressao alta");
    assert_eq!(semantic, SemanticStatusView::Succeeded);

    // The worker goes away entirely.
    world.worker.stop();
    let second = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let (status, semantic, candidates) = world.ask(&second, "pressao alta");
    assert_eq!(status, Status::Ok);
    // The cache is still valid and compatible, but the *query* cannot be
    // embedded without a worker — so the channel is unavailable and the answer
    // is lexical, which is exactly what §42 describes.
    assert_eq!(semantic, SemanticStatusView::Unavailable);
    assert!(candidates > 0);

    // And the cache is still readable: a failed pass did not damage it.
    let space = space::space_for(ProviderId::OpenAi, MODEL, DIMENSION);
    assert!(cache::load(
        &world.cache_root,
        &space,
        noteit_core::chunking::CHUNKER_VERSION
    )
    .is_ok());
}

// ================================================================ privacy

#[test]
fn no_vector_and_no_provenance_reaches_the_answer() {
    // §51: the MCP surface does not grow because the provider did.
    let world = World::new(Duration::ZERO, 0.0);
    let store = world.store(world.session(SemanticFallbackPolicy::Automatic));
    let answer = answer_for(&store, "pressao alta");
    let published = serde_json::to_string(&answer).expect("serialise");
    for forbidden in [
        "source_revision",
        "revision",
        "chunk_id",
        "vector",
        "embedding",
        "similarity",
        "score",
        "confidence",
        "openai",
        "api",
        "socket",
        "cache",
        ".sock",
    ] {
        assert!(
            !published.to_lowercase().contains(forbidden),
            "the answer publishes `{forbidden}`: {published}"
        );
    }
}
