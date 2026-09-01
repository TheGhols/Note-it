use noteit_core::metadata::{NoteMetadata, NoteProperty};
use noteit_core::model::NoteDocument;
use noteit_core::storage::StorageManager;
use noteit_core::Uuid;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn noteit_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_noteit"))
}

/// Runs the `noteit` binary in a sanitized environment without graphical/session variables.
fn run_headless(
    args: &[&str],
    xdg_dirs: Option<(&Path, &Path, &Path, &Path, &Path)>,
    no_color: bool,
) -> (i32, String, String) {
    let mut cmd = Command::new(noteit_bin());
    cmd.args(args);

    // Strip GUI/session environment variables
    cmd.env_remove("DISPLAY");
    cmd.env_remove("WAYLAND_DISPLAY");
    cmd.env_remove("DBUS_SESSION_BUS_ADDRESS");

    if no_color {
        cmd.env("NO_COLOR", "1");
    } else {
        cmd.env_remove("NO_COLOR");
    }

    if let Some((data, config, state, cache, runtime)) = xdg_dirs {
        cmd.env("XDG_DATA_HOME", data);
        cmd.env("XDG_CONFIG_HOME", config);
        cmd.env("XDG_STATE_HOME", state);
        cmd.env("XDG_CACHE_HOME", cache);
        // The writer lease and the control socket live in the runtime
        // directory, so a test that writes must be given a throwaway one too.
        // Without this, a synthetic store would leave a lock and a socket in
        // the real `$XDG_RUNTIME_DIR` — test debris in the session the person
        // is actually using.
        cmd.env("XDG_RUNTIME_DIR", runtime);
    }

    let output = cmd.output().expect("execute noteit binary");
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

fn compute_directory_fingerprints(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut map = BTreeMap::new();
    if !root.exists() {
        return map;
    }

    for entry in walkdir(root) {
        if entry.is_file() {
            let rel = entry
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let bytes = fs::read(&entry).unwrap();
            map.insert(rel, bytes);
        }
    }
    map
}

fn walkdir(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn noteit_without_arguments_shows_welcome_screen_headless() {
    let (code, stdout, stderr) = run_headless(&[], None, true);
    assert_eq!(code, 0, "Exit code should be 0");
    assert!(stderr.is_empty(), "Stderr should be empty on normal output");
    assert!(stdout.contains("Note-it"));
    assert!(stdout.contains("Suas notas, também pelo terminal."));
    assert!(stdout.contains("listar"));
    assert!(stdout.contains("ler"));
    assert!(stdout.contains("buscar"));
    assert!(stdout.contains("tags"));
    assert!(stdout.contains("propriedades"));
    assert!(stdout.contains("tarefas"));
    assert!(stdout.contains("lixeira"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("Use `noteit ajuda` para começar."));
}

#[test]
fn help_commands_and_flags_agree_and_exit_zero() {
    let (_, ajuda_stdout, _) = run_headless(&["ajuda"], None, true);
    let (help_code, help_stdout, _) = run_headless(&["help"], None, true);
    let (flag_code, flag_stdout, _) = run_headless(&["--help"], None, true);
    let (short_code, short_stdout, _) = run_headless(&["-h"], None, true);

    assert_eq!(help_code, 0);
    assert_eq!(flag_code, 0);
    assert_eq!(short_code, 0);

    assert_eq!(ajuda_stdout, help_stdout);
    assert_eq!(ajuda_stdout, flag_stdout);
    assert_eq!(ajuda_stdout, short_stdout);

    assert!(ajuda_stdout.contains("Note-it CLI"));
    assert!(ajuda_stdout.contains("noteit <comando> [opções]"));
    assert!(ajuda_stdout.contains("listar"));
    assert!(ajuda_stdout.contains("ler <ID>"));
    assert!(ajuda_stdout.contains("buscar <Q>"));
    assert!(ajuda_stdout.contains("tags"));
    assert!(ajuda_stdout.contains("propriedades"));
    assert!(ajuda_stdout.contains("tarefas"));
    assert!(ajuda_stdout.contains("lixeira"));
    assert!(ajuda_stdout.contains("status"));
}

#[test]
fn subcommand_help_flags_exit_zero() {
    for subcmd in &["listar", "ler", "buscar", "tarefas"] {
        let (code, stdout, stderr) = run_headless(&[subcmd, "--help"], None, true);
        assert_eq!(code, 0, "{subcmd} --help must exit 0");
        assert!(stderr.is_empty());
        assert!(!stdout.is_empty());
    }
}

#[test]
fn version_commands_and_flags_agree_and_exit_zero() {
    let expected = format!("Note-it {}\n", env!("CARGO_PKG_VERSION"));

    let (versao_code, versao_stdout, _) = run_headless(&["versao"], None, true);
    let (version_code, version_stdout, _) = run_headless(&["version"], None, true);
    let (flag_code, flag_stdout, _) = run_headless(&["--version"], None, true);
    let (short_code, short_stdout, _) = run_headless(&["-V"], None, true);

    assert_eq!(versao_code, 0);
    assert_eq!(version_code, 0);
    assert_eq!(flag_code, 0);
    assert_eq!(short_code, 0);

    assert_eq!(versao_stdout, expected);
    assert_eq!(version_stdout, expected);
    assert_eq!(flag_stdout, expected);
    assert_eq!(short_stdout, expected);
}

#[test]
fn status_with_empty_xdg_creates_zero_files_or_directories() {
    let tmp = tempdir().expect("tempdir");
    let data = tmp.path().join("empty_data");
    let config = tmp.path().join("empty_config");
    let state = tmp.path().join("empty_state");
    let cache = tmp.path().join("empty_cache");
    let runtime = tmp.path().join("runtime");

    assert!(!data.exists());
    assert!(!config.exists());
    assert!(!state.exists());
    assert!(!cache.exists());

    let (code, stdout, stderr) = run_headless(
        &["status"],
        Some((&data, &config, &state, &cache, &runtime)),
        true,
    );

    assert_eq!(code, 0, "status should exit 0 even on missing store");
    assert!(stderr.is_empty(), "stderr should be empty");
    assert!(stdout.contains("CLI       pronta"));
    assert!(stdout.contains("Core      disponível"));
    assert!(stdout.contains("Store     ainda não criado"));

    assert!(!data.exists(), "data dir must NOT be created by status");
    assert!(!config.exists(), "config dir must NOT be created by status");
    assert!(!state.exists(), "state dir must NOT be created by status");
    assert!(!cache.exists(), "cache dir must NOT be created by status");
}

#[test]
fn read_api_on_empty_store_returns_success_and_creates_zero_files() {
    let tmp = tempdir().expect("tempdir");
    let data = tmp.path().join("empty_data");
    let config = tmp.path().join("empty_config");
    let state = tmp.path().join("empty_state");
    let cache = tmp.path().join("empty_cache");
    let runtime = tmp.path().join("runtime");

    let xdg = Some((
        data.as_path(),
        config.as_path(),
        state.as_path(),
        cache.as_path(),
        runtime.as_path(),
    ));

    // listar
    let (code, stdout, stderr) = run_headless(&["listar"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhuma nota encontrada."));

    // buscar
    let (code, stdout, stderr) = run_headless(&["buscar", "teste"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhuma nota encontrada."));

    // tags
    let (code, stdout, stderr) = run_headless(&["tags"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhuma tag encontrada."));

    // propriedades
    let (code, stdout, stderr) = run_headless(&["propriedades"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhuma propriedade encontrada."));

    // tarefas
    let (code, stdout, stderr) = run_headless(&["tarefas"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("Nenhuma tarefa pendente."));

    // lixeira
    let (code, stdout, stderr) = run_headless(&["lixeira"], xdg, true);
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("A lixeira está vazia."));

    // ler missing on empty store
    let (code, stdout, stderr) = run_headless(&["ler", "8c4f1a2b"], xdg, true);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(stderr.contains("Erro:"));

    // Verify absolutely nothing created
    assert!(!data.exists());
    assert!(!config.exists());
    assert!(!state.exists());
    assert!(!cache.exists());
}

fn setup_rich_synthetic_store(
    tmp: &Path,
) -> (
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    PathBuf,
    Uuid,
    Uuid,
    Uuid,
) {
    let data = tmp.join("data/note-it");
    let config = tmp.join("config/note-it");
    let state = tmp.join("state/note-it");
    let cache = tmp.join("cache/note-it");
    let runtime = tmp.join("runtime");
    fs::create_dir_all(&cache).unwrap();

    let storage = StorageManager::with_custom_paths(
        data.join("notes"),
        config.clone(),
        state.clone(),
        tmp.join("runtime/note-it"),
    )
    .expect("setup storage");

    // Note 1: Medicina + PBL + Tasks
    let mut n1 = NoteDocument::new_empty();
    n1.content = "\
# Choque distributivo

- [ ] Revisar noradrenalina
- [x] Ler protocolo de sepse <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->
  - [ ] Subtarefa aninhada

```markdown
- [ ] Fake task in code fence
```
"
    .to_string();
    n1.user_metadata = NoteMetadata::try_new(
        ["Medicina".into(), "PBL".into()],
        [
            NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            },
            NoteProperty {
                key: "status".into(),
                value: "revisar".into(),
            },
        ],
    )
    .unwrap();
    storage.save_note_atomic(&n1).unwrap();

    // Note 2: Projeto GustavoOS with completed task without date
    let mut n2 = NoteDocument::new_empty();
    n2.content = "\
# Ideias GustavoOS

- [x] Concluído sem data
"
    .to_string();
    n2.user_metadata = NoteMetadata::try_new(["Projeto".into()], []).unwrap();
    storage.save_note_atomic(&n2).unwrap();

    // Note 3: Note with acentos and terminal control escapes
    let mut n3 = NoteDocument::new_empty();
    n3.content = "\
# Biópsia & Coração 🎉

Texto com \x1b[2Jescape perigoso e \x1b]52;c;Y29waWVk\x07clipboard.
\x07Alarme e \x08backspace.
"
    .to_string();
    storage.save_note_atomic(&n3).unwrap();

    // Note 4: Trash note
    let mut n4 = NoteDocument::new_empty();
    n4.content = "# PBL antigo para lixeira\n".to_string();
    storage.save_note_atomic(&n4).unwrap();
    storage.move_note_to_trash(&n4.metadata.id).unwrap();

    (
        tmp.join("data"),
        tmp.join("config"),
        tmp.join("state"),
        tmp.join("cache"),
        runtime,
        n1.metadata.id,
        n2.metadata.id,
        n3.metadata.id,
    )
}

#[test]
fn read_api_commands_and_aliases_function_on_synthetic_store() {
    let tmp = tempdir().expect("tempdir");
    let (data, config, state, cache, runtime, id1, _id2, id3) =
        setup_rich_synthetic_store(tmp.path());
    let xdg = Some((
        data.as_path(),
        config.as_path(),
        state.as_path(),
        cache.as_path(),
        runtime.as_path(),
    ));

    // 1. listar / list
    let (code, stdout, _) = run_headless(&["listar"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));
    assert!(stdout.contains("Ideias GustavoOS"));
    assert!(stdout.contains("Biópsia & Coração"));

    let (code_en, stdout_en, _) = run_headless(&["list"], xdg, true);
    assert_eq!(code_en, 0);
    assert_eq!(stdout, stdout_en);

    // Filter by tag
    let (code, stdout, _) = run_headless(&["listar", "--tag", "Medicina"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));
    assert!(!stdout.contains("Ideias GustavoOS"));

    // Filter by repeated tag AND
    let (code, stdout, _) =
        run_headless(&["listar", "--tag", "Medicina", "--tag", "PBL"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));

    // Filter by property
    let (code, stdout, _) = run_headless(
        &["listar", "--propriedade", "disciplina=cardiologia"],
        xdg,
        true,
    );
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));
    assert!(!stdout.contains("Ideias GustavoOS"));

    // Filter by limit
    let (code, stdout, _) = run_headless(&["listar", "--limite", "1"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("1 nota"));

    // 2. ler / read by 8-char prefix
    let prefix1 = &id1.to_string()[..8];
    let (code, stdout, _) = run_headless(&["ler", prefix1], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));
    assert!(stdout.contains("Medicina · PBL"));
    assert!(stdout.contains("disciplina"));
    assert!(stdout.contains("cardiologia"));

    let (code_en, stdout_en, _) = run_headless(&["read", prefix1], xdg, true);
    assert_eq!(code_en, 0);
    assert_eq!(stdout, stdout_en);

    // 3. buscar / search
    let (code, stdout, _) = run_headless(&["buscar", "noradrenalina"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Choque distributivo"));
    assert!(stdout.contains("1 ocorrência"));

    let (code_en, stdout_en, _) = run_headless(&["search", "noradrenalina"], xdg, true);
    assert_eq!(code_en, 0);
    assert_eq!(stdout, stdout_en);

    // Accent insensitive search
    let (code, stdout, _) = run_headless(&["buscar", "biopsia"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Biópsia & Coração"));

    // 4. tags
    let (code, stdout, _) = run_headless(&["tags"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Medicina"));
    assert!(stdout.contains("PBL"));
    assert!(stdout.contains("Projeto"));

    // 5. propriedades / properties
    let (code, stdout, _) = run_headless(&["propriedades"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("disciplina"));
    assert!(stdout.contains("status"));

    let (code_en, stdout_en, _) = run_headless(&["properties"], xdg, true);
    assert_eq!(code_en, 0);
    assert_eq!(stdout, stdout_en);

    // 6. tarefas / tasks
    // Pending
    let (code, stdout, _) = run_headless(&["tarefas"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Revisar noradrenalina"));
    assert!(stdout.contains("Subtarefa aninhada"));
    assert!(!stdout.contains("Ler protocolo de sepse"));
    assert!(!stdout.contains("Fake task in code fence"));

    // Completed
    let (code, stdout, _) = run_headless(&["tarefas", "--estado", "concluidas"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Ler protocolo de sepse"));
    assert!(stdout.contains("Concluído sem data"));
    assert!(!stdout.contains("Revisar noradrenalina"));

    // All
    let (code, stdout, _) = run_headless(&["tasks", "--state", "all"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Revisar noradrenalina"));
    assert!(stdout.contains("Ler protocolo de sepse"));

    // 7. lixeira / trash
    let (code, stdout, _) = run_headless(&["lixeira"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("PBL antigo para lixeira"));

    let (code_en, stdout_en, _) = run_headless(&["trash"], xdg, true);
    assert_eq!(code_en, 0);
    assert_eq!(stdout, stdout_en);

    // 8. Terminal sanitization verification on ler
    let prefix3 = &id3.to_string()[..8];
    let (code, stdout, _) = run_headless(&["ler", prefix3], xdg, true);
    assert_eq!(code, 0);
    assert!(!stdout.contains("\x1b[2J"));
    assert!(!stdout.contains("\x1b]52"));
    assert!(!stdout.contains("\x07"));
    assert!(!stdout.contains("\x08"));
    assert!(stdout.contains("Texto com escape perigoso e clipboard."));
}

#[test]
fn test_read_only_e2e_synthetic_store_byte_for_byte_unchanged() {
    let tmp = tempdir().expect("tempdir");
    let (data, config, state, cache, runtime, id1, _, _) = setup_rich_synthetic_store(tmp.path());
    let xdg = Some((
        data.as_path(),
        config.as_path(),
        state.as_path(),
        cache.as_path(),
        runtime.as_path(),
    ));

    let before_fp = compute_directory_fingerprints(tmp.path());

    // Execute every command in the Read API
    let prefix1 = &id1.to_string()[..8];
    let _ = run_headless(&["listar"], xdg, true);
    let _ = run_headless(&["list", "--limit", "2"], xdg, true);
    let _ = run_headless(&["listar", "--tag", "Medicina"], xdg, true);
    let _ = run_headless(
        &["listar", "--propriedade", "disciplina=cardiologia"],
        xdg,
        true,
    );
    let _ = run_headless(&["ler", prefix1], xdg, true);
    let _ = run_headless(&["read", &id1.to_string()], xdg, true);
    let _ = run_headless(&["buscar", "sepse"], xdg, true);
    let _ = run_headless(&["search", "noradrenalina"], xdg, true);
    let _ = run_headless(&["tags"], xdg, true);
    let _ = run_headless(&["propriedades"], xdg, true);
    let _ = run_headless(&["properties"], xdg, true);
    let _ = run_headless(&["tarefas", "--estado", "todas"], xdg, true);
    let _ = run_headless(&["tasks", "--state", "pending"], xdg, true);
    let _ = run_headless(&["lixeira"], xdg, true);
    let _ = run_headless(&["trash"], xdg, true);
    let _ = run_headless(&["status"], xdg, true);
    let _ = run_headless(&["versao"], xdg, true);
    let _ = run_headless(&["ajuda"], xdg, true);

    let after_fp = compute_directory_fingerprints(tmp.path());

    assert_eq!(
        before_fp, after_fp,
        "SYNTHETIC STORE BYTE-FOR-BYTE UNCHANGED: Read API must never mutate the store"
    );
}

#[test]
fn test_terminal_safety_e2e_with_injected_escapes() {
    let tmp = tempdir().expect("tempdir");
    let (data, config, state, cache, runtime, _id1, _, _) = setup_rich_synthetic_store(tmp.path());
    let xdg = Some((
        data.as_path(),
        config.as_path(),
        state.as_path(),
        cache.as_path(),
        runtime.as_path(),
    ));

    // 1. Injected escape in search query - query is sent raw to Core, presentation neutralizes escapes
    let malicious_query = "\x1b]52;c;AAAA\x07\x1b[2Jnoradrenalina";
    let (code, stdout, stderr) = run_headless(&["buscar", malicious_query], xdg, true);
    assert_eq!(code, 0);
    assert!(!stdout.contains("\x1b]52"));
    assert!(!stdout.contains("\x1b[2J"));
    assert!(!stdout.contains("\x07"));
    assert!(stdout.contains("Nenhuma nota encontrada."));
    assert!(stderr.is_empty());

    // 2. Injected escape in invalid subcommand
    let malicious_cmd = "\x1b[2Jcomando-malicioso";
    let (code, stdout, stderr) = run_headless(&[malicious_cmd], xdg, true);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!stderr.contains("\x1b[2J"));
    assert!(stderr.contains("comando desconhecido `comando-malicioso`"));

    // 3. Injected escape in invalid flag
    let malicious_flag = "--\x1b]52;c;AAAA\x07opcao-maliciosa";
    let (code, stdout, stderr) = run_headless(&[malicious_flag], xdg, true);
    assert_eq!(code, 2);
    assert!(stdout.is_empty());
    assert!(!stderr.contains("\x1b]52"));
    assert!(!stderr.contains("\x07"));

    // 4. Injected escape in note selector
    let malicious_selector = "\x1b[2J1234";
    let (code, stdout, stderr) = run_headless(&["ler", malicious_selector], xdg, true);
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(!stderr.contains("\x1b[2J"));
    assert!(stderr.contains("formato de seletor inválido `1234`"));

    // 5. Injected escape in custom XDG path displayed in status
    let malicious_data = tmp.path().join("data\x1b[2Jevil");
    let (code, stdout, stderr) = run_headless(
        &["status"],
        Some((
            &malicious_data,
            config.as_path(),
            state.as_path(),
            cache.as_path(),
            runtime.as_path(),
        )),
        true,
    );
    assert_eq!(code, 0);
    assert!(!stdout.contains("\x1b[2J"));
    assert!(stderr.is_empty());
}

#[test]
fn test_corrupted_note_in_store_emits_warning_to_stderr_and_lists_valid_notes_on_stdout() {
    let tmp = tempdir().expect("tempdir");
    let data = tmp.path().join("data/note-it");
    let config = tmp.path().join("config/note-it");
    let state = tmp.path().join("state/note-it");
    let cache = tmp.path().join("cache/note-it");
    fs::create_dir_all(&cache).unwrap();

    let storage = StorageManager::with_custom_paths(
        data.join("notes"),
        config.clone(),
        state.clone(),
        tmp.path().join("runtime/note-it"),
    )
    .expect("setup storage");

    let mut valid_note = NoteDocument::new_empty();
    valid_note.content = "# Nota Válida\nConteúdo normal.".to_string();
    storage.save_note_atomic(&valid_note).unwrap();

    let id_bad = Uuid::new_v4();
    let malformed = "---\nmalformed: [unclosed yaml\n---\n\n# Quebrada\n";
    fs::write(data.join("notes").join(format!("{id_bad}.md")), malformed).unwrap();

    let xdg_data = tmp.path().join("data");
    let xdg_config = tmp.path().join("config");
    let xdg_state = tmp.path().join("state");
    let xdg_cache = tmp.path().join("cache");
    let xdg_runtime = tmp.path().join("runtime");
    let xdg = Some((
        xdg_data.as_path(),
        xdg_config.as_path(),
        xdg_state.as_path(),
        xdg_cache.as_path(),
        xdg_runtime.as_path(),
    ));

    // Listar
    let (code, stdout, stderr) = run_headless(&["listar"], xdg, true);
    assert_eq!(code, 0);
    assert!(stdout.contains("Nota Válida"));
    assert!(!stdout.contains("Quebrada"));
    assert!(stderr.contains("Aviso:"));
    assert!(stderr.contains(&id_bad.as_simple().to_string()[..8]));

    // Unfiltered search
    let (code_s, stdout_s, stderr_s) = run_headless(&["buscar", "normal"], xdg, true);
    assert_eq!(code_s, 0);
    assert!(stdout_s.contains("Nota Válida"));
    assert!(stderr_s.contains("Aviso:"));
    assert!(stderr_s.contains(&id_bad.as_simple().to_string()[..8]));

    // Filtered search
    let (code_f, stdout_f, stderr_f) =
        run_headless(&["buscar", "normal", "--tag", "NaoExiste"], xdg, true);
    assert_eq!(code_f, 0);
    assert!(stdout_f.contains("Nenhuma nota encontrada."));
    assert!(stderr_f.contains("Aviso:"));
    assert!(stderr_f.contains(&id_bad.as_simple().to_string()[..8]));
}

#[test]
fn invalid_subcommand_exits_code_two_and_writes_portuguese_to_stderr() {
    let (code, stdout, stderr) = run_headless(&["comando-inexistente"], None, true);
    assert_eq!(code, 2, "Invalid subcommand must exit with code 2");
    assert!(stdout.is_empty(), "Stdout must be empty on invalid usage");
    assert!(stderr.contains("Erro: comando desconhecido `comando-inexistente`."));
    assert!(stderr.contains("Use `noteit ajuda` para ver os comandos disponíveis."));
    assert!(!stderr.contains("unrecognized subcommand"));
    assert!(!stderr.contains("\x1b["));
}

#[test]
fn invalid_flag_exits_code_two_and_writes_portuguese_to_stderr() {
    let (code, stdout, stderr) = run_headless(&["--flag-desconhecida"], None, true);
    assert_eq!(code, 2, "Invalid flag must exit with code 2");
    assert!(stdout.is_empty(), "Stdout must be empty on invalid usage");
    assert!(stderr.contains("Erro: opção desconhecida `--flag-desconhecida`."));
    assert!(stderr.contains("Use `noteit ajuda` para ver os comandos e opções disponíveis."));
    assert!(!stderr.contains("unexpected argument"));
    assert!(!stderr.contains("\x1b["));
}

#[test]
fn unexpected_argument_on_subcommand_exits_code_two_and_writes_portuguese_to_stderr() {
    for cmd in &[
        vec!["status", "argumento-inesperado"],
        vec!["ajuda", "argumento-inesperado"],
        vec!["versao", "argumento-inesperado"],
    ] {
        let (code, stdout, stderr) = run_headless(cmd, None, true);
        assert_eq!(
            code, 2,
            "Unexpected argument on {:?} must exit with code 2",
            cmd
        );
        assert!(stdout.is_empty(), "Stdout must be empty on invalid usage");
        assert!(stderr.contains("Erro: argumento inesperado `argumento-inesperado`."));
        assert!(stderr.contains("Use `noteit ajuda` para ver o formato correto de uso."));
        assert!(!stderr.contains("\x1b["));
    }
}

#[test]
fn a_word_that_is_not_a_subcommand_of_a_grouped_command_is_named_as_such() {
    // `tags`, `propriedades`, `tarefas` and `lixeira` list when given nothing
    // and write when given a subcommand, so a word they do not know is an
    // unknown command rather than a stray argument — and it still costs
    // exactly nothing but exit code 2.
    for cmd in &[
        vec!["tags", "palavra-desconhecida"],
        vec!["propriedades", "palavra-desconhecida"],
        vec!["lixeira", "palavra-desconhecida"],
    ] {
        let (code, stdout, stderr) = run_headless(cmd, None, true);
        assert_eq!(code, 2, "{cmd:?} must exit with code 2");
        assert!(stdout.is_empty(), "Stdout must be empty on invalid usage");
        assert!(
            stderr.contains("Erro: comando desconhecido `palavra-desconhecida`."),
            "{stderr}"
        );
        assert!(!stderr.contains("\x1b["));
    }
}

#[test]
fn non_tty_or_no_color_emits_no_ansi_escape_sequences() {
    for cmd in &[
        vec![],
        vec!["ajuda"],
        vec!["versao"],
        vec!["status"],
        vec!["listar"],
        vec!["tags"],
        vec!["propriedades"],
        vec!["tarefas"],
        vec!["lixeira"],
    ] {
        let (code, stdout, stderr) = run_headless(cmd, None, true);
        assert_eq!(code, 0);
        assert!(
            !stdout.contains("\x1b["),
            "Command {:?} emitted ANSI sequences under NO_COLOR in stdout: {:?}",
            cmd,
            stdout
        );
        assert!(
            !stderr.contains("\x1b["),
            "Command {:?} emitted ANSI sequences under NO_COLOR in stderr: {:?}",
            cmd,
            stderr
        );
    }
}
