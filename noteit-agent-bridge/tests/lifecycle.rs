//! Fase 5.1A — the bridge, without a model.
//!
//! §32 of the phase: CI must never depend on the internet, on somebody's
//! account or on a real model. Every test here drives a deterministic fake AI
//! client that speaks the same JSONL contract a real one does, so what is
//! being tested is the bridge and not a service.

use noteit_agent_bridge::adapter::{Adapter, Focus, Request};
use noteit_agent_bridge::event::{AgentEvent, Failure, ProviderId};
use noteit_agent_bridge::providers::Fake;
use noteit_agent_bridge::session::Session;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// An adapter that runs a script the test wrote, with the fake's vocabulary.
struct Scripted(PathBuf);

impl Adapter for Scripted {
    fn id(&self) -> ProviderId {
        ProviderId::Fake
    }
    fn command(&self, request: &Request) -> Command {
        let mut command = Command::new(&self.0);
        command
            .arg(request.composed())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn parse(&self, line: &str) -> Option<AgentEvent> {
        Fake.parse(line)
    }
}

/// Serialises "write an executable, then fork and exec it".
///
/// These tests run in threads of one process, and each writes its own client
/// and then spawns it. Between `fork` and `exec` a child holds a copy of every
/// descriptor that was open, so a script another thread is still writing comes
/// back from `exec` as `ETXTBSY` — "Text file busy". That is this file's doing
/// and not the bridge's: in use, the AI client is an installed binary that
/// nobody is writing to. Holding one lock across both halves closes the window
/// instead of retrying through it.
static SPAWNING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Writes an executable shell script and returns its path.
fn write_script(root: &Path, name: &str, body: &str) -> PathBuf {
    let path = root.join(name);
    let mut file = std::fs::File::create(&path).unwrap();
    write!(file, "#!/usr/bin/env bash\n{body}").unwrap();
    file.sync_all().unwrap();
    drop(file);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

/// Writes a client and starts a session on it, without racing any other test.
fn start(root: &Path, body: &str, request: &Request) -> (PathBuf, Session) {
    let guard = SPAWNING.lock().unwrap_or_else(|held| held.into_inner());
    let path = write_script(root, "agent", body);
    let session = Session::start(Box::new(Scripted(path.clone())), request);
    drop(guard);
    (path, session)
}

fn ask(root: &Path, body: &str) -> Session {
    start(
        root,
        body,
        &Request::about("o que escrevi aqui?", Focus::AllNotes),
    )
    .1
}

#[test]
fn a_whole_answer_arrives_as_ordered_events() {
    let root = tempfile::tempdir().unwrap();
    let events = ask(
        root.path(),
        r#"
echo '{"type":"thinking"}'
echo '{"type":"tool","name":"noteit_context"}'
echo '{"type":"delta","text":"Você "}'
echo '{"type":"delta","text":"escreveu "}'
echo '{"type":"message","text":"Você escreveu três notas."}'
echo '{"type":"done","text":"Você escreveu três notas."}'
"#,
    )
    .collect();
    assert_eq!(
        events,
        vec![
            AgentEvent::Started {
                provider: ProviderId::Fake
            },
            AgentEvent::Thinking,
            AgentEvent::ToolUse {
                name: "noteit_context".into()
            },
            AgentEvent::Delta {
                text: "Você ".into()
            },
            AgentEvent::Delta {
                text: "escreveu ".into()
            },
            AgentEvent::Message {
                text: "Você escreveu três notas.".into()
            },
            AgentEvent::Finished {
                text: "Você escreveu três notas.".into()
            },
        ]
    );
}

#[test]
fn a_client_that_is_not_installed_is_a_message_and_not_a_crash() {
    let mut session = Session::start(
        Box::new(Scripted(PathBuf::from(
            "/nonexistent/definitely-not-a-client",
        ))),
        &Request::about("o que escrevi aqui?", Focus::AllNotes),
    );
    let events = session.collect();
    assert_eq!(events.len(), 1);
    match &events[0] {
        AgentEvent::Failed { failure } => {
            assert!(matches!(failure, Failure::ClientNotFound { .. }));
            assert!(
                failure.message().contains("não está instalado"),
                "{}",
                failure.message()
            );
        }
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn invalid_json_ends_the_session_instead_of_being_guessed_at() {
    let root = tempfile::tempdir().unwrap();
    let events = ask(
        root.path(),
        r#"
echo '{"type":"thinking"}'
echo 'isto não é json'
echo '{"type":"done","text":"nunca chega"}'
"#,
    )
    .collect();
    assert!(
        matches!(
            events.last(),
            Some(AgentEvent::Failed {
                failure: Failure::Protocol { .. }
            })
        ),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AgentEvent::Finished { text } if text == "nunca chega"
        )),
        "nothing after the unreadable line is believed: {events:?}"
    );
}

#[test]
fn a_client_that_dies_without_finishing_says_so() {
    let root = tempfile::tempdir().unwrap();
    let events = ask(
        root.path(),
        r#"
echo '{"type":"thinking"}'
exit 3
"#,
    )
    .collect();
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Failed {
            failure: Failure::Exited { code: Some(3) }
        })
    );
}

#[test]
fn a_client_that_says_nothing_at_all_is_still_an_ending() {
    let root = tempfile::tempdir().unwrap();
    let events = ask(root.path(), "exit 0\n").collect();
    assert!(
        matches!(events.last(), Some(AgentEvent::Failed { .. })),
        "{events:?}"
    );
}

#[test]
fn a_client_that_never_answers_does_not_hang_the_caller() {
    let root = tempfile::tempdir().unwrap();
    let mut session = ask(
        root.path(),
        r#"
echo '{"type":"thinking"}'
sleep 600
"#,
    );
    assert_eq!(
        session.next_event(),
        Some(AgentEvent::Started {
            provider: ProviderId::Fake
        })
    );
    assert_eq!(session.next_event(), Some(AgentEvent::Thinking));
    // Nothing more is coming, and the caller finds that out instead of
    // waiting for it.
    assert_eq!(
        session.next_event_within(Duration::from_millis(300)),
        None,
        "the interface is not blocked by a silent client"
    );
    session.cancel();
    assert!(session.is_finished());
}

#[test]
fn cancelling_ends_the_process_this_session_started_and_no_other() {
    let root = tempfile::tempdir().unwrap();
    // Records its own pid, then waits forever.
    let marker = root.path().join("pid");
    let mut session = ask(
        root.path(),
        &format!(
            r#"
echo $$ > {}
echo '{{"type":"thinking"}}'
sleep 600
"#,
            marker.display()
        ),
    );
    assert!(session.next_event().is_some());
    assert_eq!(session.next_event(), Some(AgentEvent::Thinking));

    let pid: i32 = std::fs::read_to_string(&marker)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    session.cancel();
    drop(session);

    // The exact process, by the pid it wrote down — no name, no pattern.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let alive = Path::new(&format!("/proc/{pid}")).exists();
        if !alive {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the client survived cancellation (pid {pid})"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn dropping_a_session_leaves_no_process_behind() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("pid");
    {
        let mut session = ask(
            root.path(),
            &format!("echo $$ > {}\nsleep 600\n", marker.display()),
        );
        assert!(session.next_event().is_some());
        // Wait for the child to have written its pid before dropping.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            assert!(std::time::Instant::now() < deadline, "the client started");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let pid: i32 = std::fs::read_to_string(&marker)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while Path::new(&format!("/proc/{pid}")).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "closing a chat leaked an AI client (pid {pid})"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_client_is_started_with_no_terminal_of_its_own() {
    // §18: no pseudoterminal, no terminal emulator, nothing on screen. The
    // client's own view of its standard streams is the proof: a pipe, not a
    // tty.
    let root = tempfile::tempdir().unwrap();
    let events = ask(
        root.path(),
        r#"
if [ -t 0 ] || [ -t 1 ] || [ -t 2 ]; then
  echo '{"type":"done","text":"TTY"}'
else
  echo '{"type":"done","text":"PIPE"}'
fi
"#,
    )
    .collect();
    assert_eq!(
        events.last(),
        Some(&AgentEvent::Finished {
            text: "PIPE".into()
        }),
        "the AI client must never be given a terminal"
    );
}

#[test]
fn the_focus_reaches_the_client_as_part_of_the_question() {
    let root = tempfile::tempdir().unwrap();
    let (_, mut session) = start(
        root.path(),
        r#"printf '{"type":"done","text":%s}\n' "$(printf '%s' "$1" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')"
"#,
        &Request::about(
            "resuma",
            Focus::ThisNote {
                id: "abc-123".into(),
            },
        ),
    );
    let events = session.collect();
    match events.last() {
        Some(AgentEvent::Finished { text }) => {
            assert!(text.contains("abc-123"), "the note's id reached it: {text}");
            assert!(text.contains("resuma"), "and so did the question: {text}");
        }
        other => panic!("expected the echoed prompt, got {other:?}"),
    }
}
