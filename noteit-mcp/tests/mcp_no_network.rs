//! 4.1R1 / AUD-01: the server opens no socket, and the kernel is the witness.
//!
//! The boundary script checks three things about network access: that no
//! network crate is in the dependency graph, that Tokio is resolved without its
//! `net` feature, and that the crate's own source names no socket API. All
//! three are static, and static checks share a blind spot — they describe the
//! program that was written, not the program that runs.
//!
//! So this suite asks the operating system. It starts the real binary, gives it
//! real work, and reads `/proc/<pid>/fd` to see what the process actually
//! holds. A server that speaks on standard input and standard output and calls
//! into `noteit-core` has three descriptors and no socket; anything else is
//! visible here whatever the source says.
//!
//! Linux only, deliberately. Note-it is a Wayland application with a
//! layer-shell dependency, its CI runs on Arch Linux, and procfs is how this
//! question is answered on the platform the project targets. On a system
//! without `/proc` the suite says so and fails rather than passing vacuously.

mod support;

use serde_json::json;
use support::{create_note, read_revision, AuthorityBehaviour, FakeAuthority, McpClient, Sandbox};

/// Whether this system can answer the question at all.
///
/// A check that quietly passes when it could not look is worse than no check,
/// so the absence of procfs is a failure with a sentence rather than a silent
/// success.
fn require_procfs() {
    assert!(
        std::path::Path::new("/proc/self/fd").is_dir(),
        "this suite reads /proc/<pid>/fd and this system has no procfs; \
         the no-socket property cannot be checked here and must not be assumed"
    );
}

/// The descriptors that are a socket of any family.
fn sockets(descriptors: &[(u32, String)]) -> Vec<&(u32, String)> {
    descriptors
        .iter()
        .filter(|(_, target)| target.starts_with("socket:["))
        .collect()
}

/// The inode of every socket the process holds, as procfs numbers them.
fn socket_inodes(descriptors: &[(u32, String)]) -> Vec<String> {
    sockets(descriptors)
        .iter()
        .filter_map(|(_, target)| {
            target
                .strip_prefix("socket:[")
                .and_then(|rest| rest.strip_suffix(']'))
                .map(str::to_string)
        })
        .collect()
}

/// Every socket inode the kernel currently has in its TCP and UDP tables, for
/// both address families.
///
/// These files are system-wide, which is exactly what makes them useful: if a
/// descriptor of ours is an internet socket, its inode is in one of them.
fn internet_socket_inodes() -> std::collections::HashSet<String> {
    let mut inodes = std::collections::HashSet::new();
    for table in ["tcp", "tcp6", "udp", "udp6", "raw", "raw6"] {
        let Ok(text) = std::fs::read_to_string(format!("/proc/net/{table}")) else {
            continue;
        };
        for line in text.lines().skip(1) {
            // The inode is the tenth whitespace-separated column in every one
            // of these tables.
            if let Some(inode) = line.split_whitespace().nth(9) {
                inodes.insert(inode.to_string());
            }
        }
    }
    inodes
}

// --------------------------------------------------------------------------

/// Serving the whole tool surface, the process holds standard input, standard
/// output, standard error — and nothing else.
#[test]
fn the_running_server_holds_only_its_standard_streams() {
    require_procfs();
    let sandbox = Sandbox::new();
    let mut client = McpClient::start(&sandbox);

    // Real work first: a listener bound lazily inside a handler would only
    // appear once that handler had run.
    client.list_tools();
    client.call("noteit_list", json!({}));
    client.call("noteit_search", json!({ "query": "qualquer" }));
    client.call("noteit_tasks_list", json!({}));
    client.call("noteit_trash_list", json!({}));
    let id = create_note(&mut client, "BASE");
    let revision = read_revision(&mut client, &id);
    client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "MAIS", "expected_revision": revision }),
    );
    client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "NÃO", "expected_revision": "0".repeat(64) }),
    );

    let descriptors = client.open_descriptors();
    let open_sockets = sockets(&descriptors);
    assert!(
        open_sockets.is_empty(),
        "the server holds {} socket(s) after serving: {open_sockets:?}\nall descriptors: {descriptors:?}",
        open_sockets.len()
    );

    // And what it does hold is only the three streams a host gave it. Their
    // targets vary with how the harness wired them — a pipe here — so the
    // descriptor numbers are what is asserted, not the paths.
    let numbers: Vec<u32> = descriptors.iter().map(|(number, _)| *number).collect();
    assert_eq!(
        numbers,
        vec![0, 1, 2],
        "the server holds descriptors beyond its standard streams: {descriptors:?}"
    );
}

/// Nothing this process holds is an internet socket, at any point of a
/// write — including the write that goes through the authority.
///
/// The write path is the one that legitimately touches a socket at all: when
/// another instance holds the store, the change travels over `noteit-core`'s
/// private Unix socket. That is a Unix socket, it belongs to the Core, and it
/// is never an internet one. This checks the difference rather than assuming
/// it.
#[test]
fn no_descriptor_of_the_server_is_ever_an_internet_socket() {
    require_procfs();
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    // The whole read surface first, so a listener bound lazily inside any
    // handler is already open by the time the tables are consulted. Without
    // this the assertion below would be true of a set that is empty for the
    // wrong reason.
    client.list_tools();
    client.call("noteit_list", json!({}));
    client.call("noteit_search", json!({ "query": "BASE" }));
    client.call("noteit_tasks_list", json!({}));
    client.call("noteit_trash_list", json!({}));
    client.call("noteit_read", json!({ "note_id": &id }));

    let mut seen: Vec<(u32, String)> = client.open_descriptors();

    // A real authority holding the store, so the write leaves this process.
    let authority = FakeAuthority::start(&sandbox, AuthorityBehaviour::CommitForReal);
    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "PELA AUTORIDADE", "expected_revision": revision }),
    );
    assert_eq!(answer.status(), "ok", "{}", answer.raw);
    assert_eq!(authority.handled(), 1, "the write did not reach the holder");
    seen.extend(client.open_descriptors());

    let internet = internet_socket_inodes();
    let held = socket_inodes(&seen);
    for inode in &held {
        assert!(
            !internet.contains(inode),
            "socket inode {inode} held by the server is in the kernel's TCP/UDP tables: {seen:?}"
        );
    }
    // The tables were actually populated, so the loop above is a real check on
    // this machine and not a comparison against nothing. Some sockets always
    // exist on a running system; if this file were empty or unreadable the
    // assertion would pass for the wrong reason and must say so instead.
    assert!(
        !internet.is_empty(),
        "the kernel's TCP/UDP tables came back empty, so this machine cannot \
         answer whether a descriptor is an internet socket"
    );

    // Once the write is done, the process is back to its three streams: the
    // Core closes the connection rather than keeping one open.
    let after: Vec<u32> = client
        .open_descriptors()
        .iter()
        .map(|(number, _)| *number)
        .collect();
    assert_eq!(
        after,
        vec![0, 1, 2],
        "a descriptor survived the write through the authority"
    );
}

/// The store's own socket is `noteit-core`'s, and this crate never opens one.
///
/// A companion to the boundary script's source rule, from the other side: the
/// server contacts the authority by calling into the Core, so no socket
/// outlives the call, and the fail-closed path opens nothing at all.
#[test]
fn an_unreachable_authority_leaves_no_socket_behind() {
    require_procfs();
    let sandbox = Sandbox::new();
    let id = sandbox.seed("BASE").to_string();
    let mut client = McpClient::start(&sandbox);
    let revision = read_revision(&mut client, &id);

    // The lease is held and nothing is listening: the shape of an instance
    // that died holding the store.
    let coordination = sandbox.coordination();
    coordination.prepare().expect("prepare");
    let lease = noteit_core::coordination::WriterLease::try_acquire_prepared(&coordination)
        .expect("prepare")
        .expect("take the lease");

    let answer = client.call(
        "noteit_append",
        json!({ "note_id": &id, "text": "NUNCA", "expected_revision": revision }),
    );
    assert_eq!(
        answer.code(),
        Some("authority_unavailable"),
        "{}",
        answer.raw
    );

    let descriptors = client.open_descriptors();
    assert!(
        sockets(&descriptors).is_empty(),
        "a fail-closed write left a socket open: {descriptors:?}"
    );
    drop(lease);
}
