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
    xdg_dirs: Option<(&Path, &Path, &Path, &Path)>,
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

    if let Some((data, config, state, cache)) = xdg_dirs {
        cmd.env("XDG_DATA_HOME", data);
        cmd.env("XDG_CONFIG_HOME", config);
        cmd.env("XDG_STATE_HOME", state);
        cmd.env("XDG_CACHE_HOME", cache);
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
    assert!(stdout.contains("ajuda      Ver comandos"));
    assert!(stdout.contains("status     Verificar a instalação"));
    assert!(stdout.contains("versao     Mostrar versão"));
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
    assert!(ajuda_stdout.contains("ajuda       Mostrar esta ajuda"));
    assert!(ajuda_stdout.contains("versao      Mostrar a versão"));
    assert!(ajuda_stdout.contains("status      Verificar o ambiente do Note-it"));
    assert!(ajuda_stdout.contains("help        ajuda"));
    assert!(ajuda_stdout.contains("version     versao"));
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

    // Do NOT create the directories on disk
    assert!(!data.exists());
    assert!(!config.exists());
    assert!(!state.exists());
    assert!(!cache.exists());

    let (code, stdout, stderr) =
        run_headless(&["status"], Some((&data, &config, &state, &cache)), true);

    assert_eq!(code, 0, "status should exit 0 even on missing store");
    assert!(stderr.is_empty(), "stderr should be empty");
    assert!(stdout.contains("CLI       pronta"));
    assert!(stdout.contains("Core      disponível"));
    assert!(stdout.contains("Store     ainda não criado"));
    assert!(stdout.contains(&data.join("note-it").display().to_string()));
    assert!(stdout.contains(&config.join("note-it").display().to_string()));
    assert!(stdout.contains(&state.join("note-it").display().to_string()));

    // Verify absolutely nothing was created on disk
    assert!(!data.exists(), "data dir must NOT be created by status");
    assert!(!config.exists(), "config dir must NOT be created by status");
    assert!(!state.exists(), "state dir must NOT be created by status");
    assert!(!cache.exists(), "cache dir must NOT be created by status");
}

#[test]
fn status_with_existing_store_preserves_exact_fingerprints() {
    let tmp = tempdir().expect("tempdir");
    let data = tmp.path().join("data/note-it");
    let config = tmp.path().join("config/note-it");
    let state = tmp.path().join("state/note-it");
    let cache = tmp.path().join("cache/note-it");

    let notes_dir = data.join("notes");
    fs::create_dir_all(&notes_dir).expect("create notes dir");
    fs::create_dir_all(&config).expect("create config dir");
    fs::create_dir_all(&state).expect("create state dir");
    fs::create_dir_all(&cache).expect("create cache dir");

    // Put a dummy note and files
    fs::write(notes_dir.join("test-note.md"), "# Synthetic note\n").expect("write test note");
    fs::write(config.join("config.toml"), "theme = \"dark\"\n").expect("write config");
    fs::write(state.join("state.json"), "{\"notes\":{}}\n").expect("write state");

    let before_fp = compute_directory_fingerprints(tmp.path());

    let (code, stdout, stderr) = run_headless(
        &["status"],
        Some((
            &tmp.path().join("data"),
            &tmp.path().join("config"),
            &tmp.path().join("state"),
            &tmp.path().join("cache"),
        )),
        true,
    );

    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("CLI       pronta"));
    assert!(stdout.contains("Core      disponível"));
    assert!(stdout.contains("Store     encontrado"));
    assert!(stdout.contains(&data.display().to_string()));

    let after_fp = compute_directory_fingerprints(tmp.path());
    assert_eq!(
        before_fp, after_fp,
        "Running noteit status must not alter any file or directory"
    );
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
fn non_tty_or_no_color_emits_no_ansi_escape_sequences() {
    // Normal commands
    for cmd in &[vec![], vec!["ajuda"], vec!["versao"], vec!["status"]] {
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

    // Error cases under NO_COLOR
    for cmd in &[
        vec!["batata"],
        vec!["--opcao-invalida"],
        vec!["status", "sobrando"],
    ] {
        let (code, stdout, stderr) = run_headless(cmd, None, true);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(
            !stderr.contains("\x1b["),
            "Error command {:?} emitted ANSI sequences under NO_COLOR in stderr: {:?}",
            cmd,
            stderr
        );
    }
}
