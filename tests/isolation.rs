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

#[test]
fn r009_harness_allows_stop_and_verify_from_inherited_isolated_xdg_environment() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let harness = repo_root.join("scripts/note-it-isolated");

    // Check if dbus-daemon is installed. If not, log explicit SKIPPED as mandated by Section 26.
    let dbus_daemon_check = Command::new("which").arg("dbus-daemon").output();
    let has_dbus_daemon = dbus_daemon_check
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_dbus_daemon {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: dbus-daemon not found");
        return;
    }

    let python3_check = Command::new("python3")
        .arg("-c")
        .arg("from gi.repository import Gio, GLib")
        .output();
    let has_python_gio = python3_check.map(|o| o.status.success()).unwrap_or(false);
    if !has_python_gio {
        eprintln!("TEST REGISTERED: passed by harness; SCENARIO EXECUTED: NO; REASON: python3-gi Gio not found");
        return;
    }

    let ambient_bus_address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let root_str = root.to_str().unwrap();

    // Create a stub runner that owns io.github.theghols.NoteIt on the private bus
    let stub_path = root.join("stub-noteit.py");
    let stub_code = r#"#!/usr/bin/env python3
import sys, signal
from gi.repository import Gio, GLib

if len(sys.argv) > 1 and sys.argv[1] == 'quit':
    sys.exit(0)

loop = GLib.MainLoop()

def on_name_acquired(conn, name):
    pass

def on_name_lost(conn, name):
    loop.quit()

owner_id = Gio.bus_own_name(
    Gio.BusType.SESSION,
    'io.github.theghols.NoteIt',
    Gio.BusNameOwnerFlags.NONE,
    None,
    on_name_acquired,
    on_name_lost
)

signal.signal(signal.SIGTERM, lambda *_: loop.quit())
loop.run()
"#;
    std::fs::write(&stub_path, stub_code).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // 1. Start a real isolated session via the harness in background
    let mut bg_child = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg(root_str)
        .arg("--")
        .arg("run")
        .env("NOTE_IT_BINARY", &stub_path)
        .current_dir(&repo_root)
        .spawn()
        .expect("start isolated background session");

    // 2. Wait for session/bus.address and session/bus.pid
    let session_dir = root.join("session");
    let bus_addr_file = session_dir.join("bus.address");
    let bus_pid_file = session_dir.join("bus.pid");

    let mut bus_pid: u32 = 0;
    let mut private_bus_addr = String::new();
    let start_wait = std::time::Instant::now();
    while start_wait.elapsed() < std::time::Duration::from_secs(5) {
        if bus_addr_file.exists() && bus_pid_file.exists() {
            if let Ok(addr) = std::fs::read_to_string(&bus_addr_file) {
                if let Ok(pid_str) = std::fs::read_to_string(&bus_pid_file) {
                    if let Ok(parsed_pid) = pid_str.trim().parse::<u32>() {
                        private_bus_addr = addr.trim().to_string();
                        bus_pid = parsed_pid;
                        break;
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(bus_pid > 0, "Private D-Bus session bus.pid was not created");
    assert!(
        !private_bus_addr.is_empty(),
        "Private bus.address was not created"
    );

    // Ensure private bus address differs from ambient bus
    if let Some(ref amb) = ambient_bus_address {
        assert_ne!(
            &private_bus_addr, amb,
            "Private bus address must differ from ambient bus"
        );
    }

    // Wait for the instance name to appear on the bus via verify
    let verify_start = std::time::Instant::now();
    let mut ready = false;
    while verify_start.elapsed() < std::time::Duration::from_secs(5) {
        let verify_res = Command::new("bash")
            .arg(&harness)
            .arg("--root")
            .arg(root_str)
            .arg("--verify")
            .env("NOTE_IT_BINARY", &stub_path)
            .output();
        if let Ok(out) = verify_res {
            if out.status.success() {
                ready = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        ready,
        "Isolated session did not become ready on the private bus"
    );

    // 3. Subshell inheriting isolated XDG environment and isolated HOME
    let inherited_home = root.join("home");
    let inherited_data = root.join("data");
    let inherited_config = root.join("config");
    let inherited_state = root.join("state");
    let inherited_cache = root.join("cache");

    // 4. Verify from inside inherited subshell
    let verify_output = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg(root_str)
        .arg("--verify")
        .env("HOME", &inherited_home)
        .env("XDG_DATA_HOME", &inherited_data)
        .env("XDG_CONFIG_HOME", &inherited_config)
        .env("XDG_STATE_HOME", &inherited_state)
        .env("XDG_CACHE_HOME", &inherited_cache)
        .env("NOTE_IT_BINARY", &stub_path)
        .current_dir(&repo_root)
        .output()
        .expect("verify inside inherited environment");

    let stdout_v = String::from_utf8_lossy(&verify_output.stdout);
    let stderr_v = String::from_utf8_lossy(&verify_output.stderr);
    assert_eq!(
        verify_output.status.code(),
        Some(0),
        "--verify from inherited environment must succeed with 0!\nstdout: {stdout_v}\nstderr: {stderr_v}"
    );
    assert!(
        stdout_v.contains("io.github.theghols.NoteIt is on the private bus"),
        "Verify output must identify application on private bus"
    );

    // 5. Stop from inside inherited subshell
    let stop_output = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg(root_str)
        .arg("--stop")
        .env("HOME", &inherited_home)
        .env("XDG_DATA_HOME", &inherited_data)
        .env("XDG_CONFIG_HOME", &inherited_config)
        .env("XDG_STATE_HOME", &inherited_state)
        .env("XDG_CACHE_HOME", &inherited_cache)
        .env("NOTE_IT_BINARY", &stub_path)
        .current_dir(&repo_root)
        .output()
        .expect("stop inside inherited environment");

    let stdout_s = String::from_utf8_lossy(&stop_output.stdout);
    let stderr_s = String::from_utf8_lossy(&stop_output.stderr);
    assert_eq!(
        stop_output.status.code(),
        Some(0),
        "--stop from inherited environment must succeed with 0!\nstdout: {stdout_s}\nstderr: {stderr_s}"
    );

    // 6. Verify that private bus PID is dead
    let mut pid_terminated = false;
    let term_wait = std::time::Instant::now();
    while term_wait.elapsed() < std::time::Duration::from_secs(4) {
        let kill_check = Command::new("kill")
            .arg("-0")
            .arg(bus_pid.to_string())
            .output();
        if let Ok(out) = kill_check {
            if !out.status.success() {
                pid_terminated = true;
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        pid_terminated,
        "Private D-Bus PID {bus_pid} must be terminated after --stop"
    );

    // 7. Verify session files cleaned up
    assert!(
        !bus_addr_file.exists(),
        "bus.address must be removed by --stop"
    );
    assert!(!bus_pid_file.exists(), "bus.pid must be removed by --stop");

    // 8. Negative test: subsequent --verify from inherited environment MUST FAIL (exit 92)
    let neg_verify = Command::new("bash")
        .arg(&harness)
        .arg("--root")
        .arg(root_str)
        .arg("--verify")
        .env("HOME", &inherited_home)
        .env("XDG_DATA_HOME", &inherited_data)
        .env("XDG_CONFIG_HOME", &inherited_config)
        .env("XDG_STATE_HOME", &inherited_state)
        .env("XDG_CACHE_HOME", &inherited_cache)
        .env("NOTE_IT_BINARY", &stub_path)
        .current_dir(&repo_root)
        .output()
        .expect("negative verify");

    assert_eq!(
        neg_verify.status.code(),
        Some(92),
        "Subsequent --verify must fail with exit 92 (no live isolated session)"
    );
    let neg_stderr = String::from_utf8_lossy(&neg_verify.stderr);
    assert!(
        neg_stderr.contains("no live isolated session"),
        "Negative verify stderr must report 'no live isolated session', got: {neg_stderr}"
    );

    // 9. Host safety: ensure ambient bus (if present) is untouched
    if let Some(ref amb) = ambient_bus_address {
        let amb_check = Command::new("dbus-send")
            .arg("--session")
            .arg("--dest=org.freedesktop.DBus")
            .arg("/org/freedesktop/DBus")
            .arg("org.freedesktop.DBus.GetId")
            .env("DBUS_SESSION_BUS_ADDRESS", amb)
            .output();
        if let Ok(out) = amb_check {
            assert!(
                out.status.success(),
                "Ambient bus must remain fully functional"
            );
        }
    }

    // Cleanup background child
    let _ = bg_child.kill();
    let _ = bg_child.wait();
}
