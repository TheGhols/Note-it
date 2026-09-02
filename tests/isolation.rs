//! The test harness has to be tested too.
//!
//! Phase 3.7's physical testing wrote a note into the user's own store. The
//! cause was not in the application: `scripts/note-it-isolated` overrode the
//! four XDG variables and left the session bus alone, and Note-it is a
//! single-instance `GApplication` — so with a daemon already running, the
//! "isolated" process handed its command line to that daemon over D-Bus and the
//! real store was written by the real daemon.
//!
//! `scripts/test-isolation` builds that situation deliberately and asserts the
//! harness cannot reach it. It lives in shell because what it tests is a shell
//! script and a bus, but it runs from here so that it is covered by `cargo
//! test` — one of the gates this project actually runs — rather than being a
//! thing someone has to remember.
//!
//! Where there is a display it also starts a genuine `note-it --background`
//! daemon, on a bus of its own with a store of its own, and reproduces the
//! incident end to end. In CI there is no display and that half is skipped and
//! says so; the rest, which covers every causal step, runs everywhere.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn the_isolation_harness_cannot_reach_the_ambient_session() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = repo_root.join("scripts/test-isolation");
    assert!(script.is_file(), "missing {}", script.display());

    let output = Command::new("bash")
        .arg(&script)
        .current_dir(&repo_root)
        .output()
        .expect("scripts/test-isolation should be runnable with bash");

    if !output.status.success() {
        panic!(
            "the isolation harness regression test failed ({})\n\
             --- stdout ---\n{}\n--- stderr ---\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

#[test]
fn r009_harness_rejects_path_traversal_in_root() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let harness = repo_root.join("scripts/note-it-isolated");

    let output = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg("/tmp/note-it-test-12345/../escape")
        .arg("--")
        .arg("help")
        .output()
        .expect("run harness");

    assert_eq!(output.status.code(), Some(90));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("path traversal components (..)"),
        "Harness must reject path traversal, got: {stderr}"
    );
}

#[test]
fn r009_harness_rejects_root_inside_real_home() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let harness = repo_root.join("scripts/note-it-isolated");

    let real_home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let hostile_target = format!("{real_home}/malicious_isolated_target");

    let output = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg(&hostile_target)
        .arg("--")
        .arg("help")
        .output()
        .expect("run harness");

    assert_eq!(output.status.code(), Some(90));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("inside the real XDG tree") || stderr.contains("real home directory"),
        "Harness must reject root inside real home, got: {stderr}"
    );
}

