//! Local snapshots of everything that can be recovered.
//!
//! A backup is a plain directory of plain files: the notes, the trash, the
//! configuration and the window state, copied as they are. No archive, no
//! database, no format of its own. Whatever goes wrong with Note-it, a
//! snapshot can be read with `ls` and put back with `cp`, and that is the whole
//! point of it.
//!
//! **Nothing leaves the machine.** There is no server, no cloud, no HTTP
//! client, no upload and nothing to configure that would introduce one. A
//! snapshot is written next to the notes it copies.
//!
//! **The rename is the commit point**, exactly as it is for a note (see
//! [`crate::atomic_file::write_atomic`]). A snapshot is built inside
//! `backups/.tmp.…`, and only a completed one is renamed to its final name. A
//! process that dies halfway leaves a `.tmp.…` directory, which is never a
//! snapshot: it does not have a snapshot's name and it is removed by the next
//! backup. Nothing that is not a `.tmp.…` directory is ever removed by that
//! sweep.
//!
//! **Retention runs only after a new snapshot has been committed.** Deleting an
//! old backup to make room for one that then fails would trade protection for
//! nothing, so the order is create, commit, and only then prune.
//!
//! **What a local snapshot protects against** is an accidental deletion, a
//! logical corruption, an edit to undo, a version to go back to. It sits on the
//! same disk as the notes, so it protects against **none** of: a dead drive, a
//! lost machine, a stolen one. It is not encrypted. Anyone reading this looking
//! for disaster recovery is looking at the wrong thing.

use crate::atomic_file::sync_directory_after_commit;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Never more than one automatic snapshot inside this window.
pub const AUTOMATIC_BACKUP_INTERVAL_HOURS: i64 = 24;

/// How many committed snapshots are kept. One pool, whatever made them.
pub const SNAPSHOT_RETENTION: usize = 7;

/// How long a failed automatic backup waits before it is attempted again.
///
/// Without this, a store whose backups directory cannot be written would try
/// again on every autosave — which is precisely the continuous background work
/// this phase is not allowed to introduce.
pub const AUTOMATIC_BACKUP_RETRY_MINUTES: i64 = 15;

/// The prefix that marks a directory as this routine's own scratch space.
/// Nothing without it is ever removed by the sweep.
const TEMP_PREFIX: &str = ".tmp.";

const MANIFEST_FILE: &str = "manifest.json";

/// Distinguishes the temp directory of one backup from another's within a
/// process. A crashed process leaves its own behind; the next run sweeps it.
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    Automatic,
    Manual,
}

impl SnapshotKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotKind::Automatic => "automatic",
            SnapshotKind::Manual => "manual",
        }
    }
}

/// What a snapshot says about itself. Its presence is also what makes the
/// directory a snapshot rather than a directory someone happened to create.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub kind: String,
    pub notes: usize,
    pub trash: usize,
    pub config: bool,
    pub state: bool,
}

/// One committed snapshot on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub path: PathBuf,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// The four things a snapshot copies. Everything else in the data directory —
/// the backups themselves above all — is deliberately not here.
#[derive(Debug, Clone)]
pub struct BackupSources {
    pub notes_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub config_file: PathBuf,
    pub state_file: PathBuf,
}

/// Whether an automatic backup is owed.
///
/// Pure, so the twenty-four hour rule can be tested without waiting a day.
/// A snapshot dated in the future — a clock that moved backwards — postpones
/// the next automatic backup rather than producing one per save; a manual
/// backup is always available.
pub fn automatic_backup_due(
    last_success: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    interval: Duration,
) -> bool {
    match last_success {
        None => true,
        Some(last) => now.signed_duration_since(last) >= interval,
    }
}

/// Whether a failed attempt may be retried yet.
pub fn retry_allowed(
    last_attempt: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    interval: Duration,
) -> bool {
    match last_attempt {
        None => true,
        Some(last) => now.signed_duration_since(last) >= interval,
    }
}

/// Every committed snapshot, oldest first.
///
/// A directory is a snapshot when it is a real directory — never a symlink —
/// whose name is not scratch space and which holds a readable manifest. A
/// half-written backup satisfies none of that, so it can never be listed as a
/// valid one. Reading only.
pub fn list_snapshots(backups_dir: &Path) -> Vec<Snapshot> {
    let Ok(entries) = fs::read_dir(backups_dir) else {
        return Vec::new();
    };

    let mut snapshots: Vec<Snapshot> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_dir()) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Some(manifest) = read_manifest(&path.join(MANIFEST_FILE)) else {
            continue;
        };
        snapshots.push(Snapshot {
            name: name.to_string(),
            created_at: manifest.created_at,
            path,
        });
    }

    snapshots.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    snapshots
}

/// When the last snapshot was committed, or `None` when there is none.
pub fn last_snapshot_time(backups_dir: &Path) -> Option<DateTime<Utc>> {
    list_snapshots(backups_dir)
        .last()
        .map(|snapshot| snapshot.created_at)
}

fn read_manifest(path: &Path) -> Option<SnapshotManifest> {
    serde_json::from_str::<SnapshotManifest>(&fs::read_to_string(path).ok()?).ok()
}

/// Builds a snapshot, commits it, and only then prunes the old ones.
///
/// Everything up to the rename can fail with nothing gained and nothing lost:
/// the scratch directory is removed and the store is untouched. From the
/// rename onwards the snapshot exists, and what follows — the directory sync
/// and the retention sweep — is reported as a warning rather than turned into
/// a failure that would deny a backup the disk already holds.
pub fn create_snapshot(
    backups_dir: &Path,
    sources: &BackupSources,
    kind: SnapshotKind,
    now: DateTime<Utc>,
    keep: usize,
) -> Result<PathBuf, String> {
    fs::create_dir_all(backups_dir)
        .map_err(|e| format!("Failed to create the backups directory: {e}"))?;

    // Before anything of this run exists, so it can never sweep its own work.
    remove_stale_temp_directories(backups_dir);

    let temp = backups_dir.join(format!(
        "{TEMP_PREFIX}{}.{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::SeqCst)
    ));
    let destination = match build_snapshot(&temp, sources, kind, now, backups_dir) {
        Ok(destination) => destination,
        Err(error) => {
            // Best effort: the backup has already failed, and the error worth
            // reporting is that one.
            let _ = fs::remove_dir_all(&temp);
            return Err(error);
        }
    };

    // The commit point.
    fs::rename(&temp, &destination).map_err(|error| {
        let _ = fs::remove_dir_all(&temp);
        format!(
            "Failed to commit the snapshot at {}: {error}",
            destination.display()
        )
    })?;

    // Past it. The snapshot exists from here on.
    sync_directory_after_commit(backups_dir, "the backups directory");
    for error in apply_retention(backups_dir, keep) {
        eprintln!("The snapshot was created, but an old one could not be removed: {error}");
    }

    Ok(destination)
}

/// Fills the scratch directory and works out where the snapshot will land.
/// Everything here is before the commit point.
fn build_snapshot(
    temp: &Path,
    sources: &BackupSources,
    kind: SnapshotKind,
    now: DateTime<Utc>,
    backups_dir: &Path,
) -> Result<PathBuf, String> {
    fs::create_dir(temp)
        .map_err(|e| format!("Failed to create the scratch directory for the snapshot: {e}"))?;

    let notes = copy_directory(&sources.notes_dir, &temp.join("notes"))?;
    let trash = copy_directory(&sources.trash_dir, &temp.join("trash"))?;
    let config = copy_optional_file(&sources.config_file, &temp.join("config.toml"))?;
    let state = copy_optional_file(&sources.state_file, &temp.join("state.json"))?;

    let manifest = SnapshotManifest {
        version: 1,
        created_at: now,
        kind: kind.as_str().to_string(),
        notes,
        trash,
        config,
        state,
    };
    let serialized = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize the snapshot manifest: {e}"))?;
    fs::write(temp.join(MANIFEST_FILE), serialized)
        .map_err(|e| format!("Failed to write the snapshot manifest: {e}"))?;

    // Best effort, and before the commit: the snapshot's own files are on disk
    // by the time the directory entry that names it appears.
    sync_directory_after_commit(temp, "the snapshot being built");

    reserve_snapshot_name(backups_dir, now)
}

/// The name a snapshot commits to.
///
/// Sortable by construction, so the order on disk is chronological order. Two
/// backups inside the same second are possible — a manual one right after an
/// automatic one — so a single numbered alternative is offered; beyond that
/// the backup fails rather than inventing names.
fn reserve_snapshot_name(backups_dir: &Path, now: DateTime<Utc>) -> Result<PathBuf, String> {
    let base = now.format("%Y-%m-%dT%H-%M-%SZ").to_string();
    for candidate in [base.clone(), format!("{base}-2")] {
        let path = backups_dir.join(&candidate);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(format!(
        "a snapshot for {base} already exists, and so does its alternative; nothing was written"
    ))
}

/// Copies the regular files of one directory, and only those.
///
/// A symlink is never followed and never copied. A store is a directory of
/// note files; anything else in there was not put there by Note-it, and
/// following a link out of it would let a single crafted entry make the backup
/// copy `/etc` or a home directory. Names beginning with `.` are skipped too,
/// which is what keeps a `.tmp.…` left by an interrupted save out of the
/// snapshot.
///
/// A directory that does not exist is an empty one. A path that exists and is
/// not a directory is a broken store and fails the backup, because copying
/// nothing and calling it a backup of everything is the one thing a backup may
/// never do.
fn copy_directory(source: &Path, destination: &Path) -> Result<usize, String> {
    fs::create_dir_all(destination).map_err(|e| {
        format!(
            "Failed to create {} inside the snapshot: {e}",
            destination.display()
        )
    })?;

    if !source.exists() {
        return Ok(0);
    }

    let entries = fs::read_dir(source)
        .map_err(|e| format!("Failed to read {} for the backup: {e}", source.display()))?;

    let mut copied = 0;
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("Failed to read an entry of {}: {e}", source.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
        if metadata.file_type().is_symlink() {
            eprintln!(
                "Backup skipped {}: it is a symbolic link, and a backup never follows one",
                path.display()
            );
            continue;
        }
        if !metadata.file_type().is_file() {
            continue;
        }

        fs::copy(&path, destination.join(name))
            .map_err(|e| format!("Failed to copy {} into the snapshot: {e}", path.display()))?;
        copied += 1;
    }

    Ok(copied)
}

/// Copies a single file if it is there. Reports whether it was.
fn copy_optional_file(source: &Path, destination: &Path) -> Result<bool, String> {
    let Ok(metadata) = fs::symlink_metadata(source) else {
        return Ok(false);
    };
    if metadata.file_type().is_symlink() {
        eprintln!(
            "Backup skipped {}: it is a symbolic link, and a backup never follows one",
            source.display()
        );
        return Ok(false);
    }
    if !metadata.file_type().is_file() {
        return Ok(false);
    }

    fs::copy(source, destination)
        .map_err(|e| format!("Failed to copy {} into the snapshot: {e}", source.display()))?;
    Ok(true)
}

/// Removes this routine's own leftovers, and nothing else.
///
/// Only a directory whose name begins with the scratch prefix is touched. A
/// note, a snapshot, and anything a person put in the backups directory are all
/// left exactly where they are: mistaking a user's file for debris would be a
/// worse failure than the debris.
pub fn remove_stale_temp_directories(backups_dir: &Path) {
    let Ok(entries) = fs::read_dir(backups_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(TEMP_PREFIX) {
            continue;
        }
        if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_dir()) {
            continue;
        }
        if let Err(error) = fs::remove_dir_all(&path) {
            eprintln!(
                "Failed to remove the leftover scratch directory {}: {error}",
                path.display()
            );
        }
    }
}

/// Keeps the newest `keep` snapshots and removes the rest. Returns what could
/// not be removed; a caller that has just committed a snapshot reports those
/// rather than failing over them.
fn apply_retention(backups_dir: &Path, keep: usize) -> Vec<String> {
    apply_retention_with(backups_dir, keep, |path| fs::remove_dir_all(path))
}

fn apply_retention_with(
    backups_dir: &Path,
    keep: usize,
    remove: impl Fn(&Path) -> std::io::Result<()>,
) -> Vec<String> {
    let snapshots = list_snapshots(backups_dir);
    let excess = snapshots.len().saturating_sub(keep);
    snapshots
        .into_iter()
        .take(excess)
        .filter_map(|snapshot| {
            remove(&snapshot.path)
                .err()
                .map(|error| format!("{}: {error}", snapshot.path.display()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    struct Store {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        notes: PathBuf,
        trash: PathBuf,
        config: PathBuf,
        state: PathBuf,
        backups: PathBuf,
    }

    impl Store {
        fn sources(&self) -> BackupSources {
            BackupSources {
                notes_dir: self.notes.clone(),
                trash_dir: self.trash.clone(),
                config_file: self.config.clone(),
                state_file: self.state.clone(),
            }
        }

        fn backup(&self, now: DateTime<Utc>) -> Result<PathBuf, String> {
            create_snapshot(
                &self.backups,
                &self.sources(),
                SnapshotKind::Manual,
                now,
                SNAPSHOT_RETENTION,
            )
        }
    }

    fn store() -> Store {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path().to_path_buf();
        let notes = root.join("notes");
        let trash = root.join("trash");
        let backups = root.join("backups");
        fs::create_dir_all(&notes).expect("notes");
        fs::create_dir_all(&trash).expect("trash");
        Store {
            _tmp: tmp,
            config: root.join("config.toml"),
            state: root.join("state.json"),
            root,
            notes,
            trash,
            backups,
        }
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("fixed instant")
            .with_timezone(&Utc)
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_snapshot_copies_the_notes_the_trash_the_config_and_the_state() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota A").expect("note a");
        fs::write(store.notes.join("b.md"), "nota B").expect("note b");
        fs::write(store.trash.join("c.md"), "nota C na lixeira").expect("trashed note");
        fs::write(&store.config, "theme = \"dark\"\n").expect("config");
        fs::write(&store.state, "{\"notes\":{}}").expect("state");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        assert_eq!(
            snapshot.file_name().and_then(|name| name.to_str()),
            Some("2026-08-29T09-30-00Z")
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("notes/a.md")).expect("a"),
            "nota A"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("notes/b.md")).expect("b"),
            "nota B"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("trash/c.md")).expect("c"),
            "nota C na lixeira"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("config.toml")).expect("config"),
            "theme = \"dark\"\n"
        );
        assert_eq!(
            fs::read_to_string(snapshot.join("state.json")).expect("state"),
            "{\"notes\":{}}"
        );

        let manifest = read_manifest(&snapshot.join(MANIFEST_FILE)).expect("manifest");
        assert_eq!(manifest.notes, 2);
        assert_eq!(manifest.trash, 1);
        assert!(manifest.config);
        assert!(manifest.state);
        assert_eq!(manifest.kind, "manual");
        assert_eq!(manifest.created_at, at("2026-08-29T09:30:00Z"));
    }

    #[test]
    fn a_snapshot_records_a_missing_config_and_state_rather_than_inventing_them() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        assert!(!snapshot.join("config.toml").exists());
        assert!(!snapshot.join("state.json").exists());
        let manifest = read_manifest(&snapshot.join(MANIFEST_FILE)).expect("manifest");
        assert!(!manifest.config);
        assert!(!manifest.state);
    }

    #[test]
    fn a_snapshot_never_contains_the_backups_directory() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        let first = store.backup(at("2026-08-29T09:30:00Z")).expect("first");
        let second = store.backup(at("2026-08-30T09:30:00Z")).expect("second");

        assert!(!second.join("backups").exists());
        assert!(!second.join("notes/backups").exists());
        assert!(first.exists(), "the earlier snapshot is untouched");
        // And the second one is a snapshot of the store, not of the store plus
        // its snapshots.
        assert_eq!(names_in(&second.join("notes")), vec!["a.md".to_string()]);
    }

    #[test]
    fn a_snapshot_never_contains_temp_files() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        fs::write(store.notes.join(".tmp.a.md.4242"), "meio escrita").expect("debris");
        fs::write(store.trash.join(".tmp.debris"), "resto").expect("trash debris");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        assert_eq!(names_in(&snapshot.join("notes")), vec!["a.md".to_string()]);
        assert!(names_in(&snapshot.join("trash")).is_empty());
    }

    #[test]
    fn a_snapshot_does_not_modify_the_files_it_copies() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota A").expect("note");
        fs::write(store.trash.join("c.md"), "nota C").expect("trashed");
        fs::write(&store.config, "theme = \"dark\"\n").expect("config");

        let fingerprint = |root: &Path| -> Vec<(PathBuf, u64, std::time::SystemTime)> {
            let mut files = Vec::new();
            let mut stack = vec![root.to_path_buf()];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.file_name().and_then(|n| n.to_str()) == Some("backups") {
                        continue;
                    }
                    let meta = entry.metadata().expect("metadata");
                    if meta.is_dir() {
                        stack.push(path);
                    } else {
                        files.push((path, meta.len(), meta.modified().expect("mtime")));
                    }
                }
            }
            files.sort();
            files
        };

        let before = fingerprint(&store.root);
        store.backup(at("2026-08-29T09:30:00Z")).expect("backup");
        assert_eq!(fingerprint(&store.root), before);
    }

    #[test]
    fn an_incomplete_snapshot_is_never_listed_as_valid() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        // What a process killed halfway through leaves behind.
        let half = store.backups.join(".tmp.999.1");
        fs::create_dir_all(half.join("notes")).expect("half-written scratch");
        fs::write(half.join("notes/a.md"), "metade").expect("half note");
        // And a directory with a snapshot's name but no manifest: not a
        // snapshot either.
        let nameless = store.backups.join("2020-01-01T00-00-00Z");
        fs::create_dir_all(&nameless).expect("nameless");

        let snapshots = list_snapshots(&store.backups);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "2026-08-29T09-30-00Z");
        assert_eq!(
            last_snapshot_time(&store.backups),
            Some(at("2026-08-29T09:30:00Z"))
        );

        // The next backup sweeps the scratch directory and leaves the rest.
        store.backup(at("2026-08-30T09:30:00Z")).expect("backup");
        assert!(!half.exists());
        assert!(nameless.exists());
    }

    #[test]
    fn a_backup_that_cannot_be_built_leaves_nothing_behind() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        // A file where the trash directory belongs: reading it as a directory
        // fails on path resolution, which fails for every user, root included.
        fs::remove_dir_all(&store.trash).expect("remove trash dir");
        fs::write(&store.trash, "não é um diretório").expect("occupy the trash path");

        let error = store
            .backup(at("2026-08-29T09:30:00Z"))
            .expect_err("a store that cannot be read cannot be backed up");
        assert!(error.contains("trash"), "{error}");

        assert!(list_snapshots(&store.backups).is_empty());
        assert!(
            names_in(&store.backups).is_empty(),
            "no scratch directory may survive a failed backup: {:?}",
            names_in(&store.backups)
        );
    }

    #[test]
    fn a_commit_that_cannot_land_leaves_no_snapshot_and_no_scratch() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        // Both names a snapshot for this instant could take are occupied by
        // non-empty directories, so the rename cannot land on either.
        for name in ["2026-08-29T09-30-00Z", "2026-08-29T09-30-00Z-2"] {
            fs::create_dir_all(store.backups.join(name).join("ocupado")).expect("occupy");
        }

        let error = store
            .backup(at("2026-08-29T09:30:00Z"))
            .expect_err("the snapshot cannot be committed");
        assert!(error.contains("already exists"), "{error}");

        assert!(list_snapshots(&store.backups).is_empty());
        assert!(
            !names_in(&store.backups)
                .iter()
                .any(|name| name.starts_with(TEMP_PREFIX)),
            "a failed commit left scratch behind: {:?}",
            names_in(&store.backups)
        );
    }

    #[test]
    fn two_backups_in_the_same_second_do_not_collide() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        let first = store.backup(at("2026-08-29T09:30:00Z")).expect("first");
        let second = store.backup(at("2026-08-29T09:30:00Z")).expect("second");
        assert_ne!(first, second);
        assert_eq!(list_snapshots(&store.backups).len(), 2);
    }

    #[test]
    fn symlinks_are_not_followed_outside_the_note_it_roots() {
        let store = store();
        let outside = store.root.join("fora");
        fs::create_dir_all(&outside).expect("outside dir");
        fs::write(outside.join("segredo"), "não deve ser copiado").expect("secret");

        fs::write(store.notes.join("a.md"), "nota").expect("note");
        symlink(&outside, store.notes.join("escape")).expect("directory symlink");
        symlink(outside.join("segredo"), store.notes.join("segredo.md")).expect("file symlink");
        symlink(&outside, store.trash.join("escape")).expect("trash symlink");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        assert_eq!(names_in(&snapshot.join("notes")), vec!["a.md".to_string()]);
        assert!(names_in(&snapshot.join("trash")).is_empty());
        assert!(!snapshot.join("notes/segredo.md").exists());
        assert!(!snapshot.join("notes/escape").exists());
    }

    #[test]
    fn a_symlinked_config_or_state_is_not_followed_either() {
        let store = store();
        let outside = store.root.join("fora");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(outside.join("segredo.toml"), "chave = \"secreta\"").expect("secret");
        symlink(outside.join("segredo.toml"), &store.config).expect("config symlink");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");
        assert!(!snapshot.join("config.toml").exists());
        let manifest = read_manifest(&snapshot.join(MANIFEST_FILE)).expect("manifest");
        assert!(!manifest.config);
    }

    #[test]
    fn an_automatic_backup_is_not_repeated_inside_the_24h_window() {
        let interval = Duration::hours(AUTOMATIC_BACKUP_INTERVAL_HOURS);
        let last = at("2026-08-29T09:30:00Z");

        assert!(automatic_backup_due(None, last, interval));
        assert!(!automatic_backup_due(Some(last), last, interval));
        assert!(!automatic_backup_due(
            Some(last),
            at("2026-08-30T09:29:59Z"),
            interval
        ));
    }

    #[test]
    fn a_new_backup_can_be_created_after_the_interval() {
        let interval = Duration::hours(AUTOMATIC_BACKUP_INTERVAL_HOURS);
        let last = at("2026-08-29T09:30:00Z");

        assert!(automatic_backup_due(
            Some(last),
            at("2026-08-30T09:30:00Z"),
            interval
        ));
        assert!(automatic_backup_due(
            Some(last),
            at("2026-09-05T00:00:00Z"),
            interval
        ));
        // A clock that moved backwards postpones rather than repeating.
        assert!(!automatic_backup_due(
            Some(at("2027-01-01T00:00:00Z")),
            last,
            interval
        ));
    }

    #[test]
    fn a_failed_attempt_waits_before_being_retried() {
        let interval = Duration::minutes(AUTOMATIC_BACKUP_RETRY_MINUTES);
        let attempt = at("2026-08-29T09:30:00Z");
        assert!(retry_allowed(None, attempt, interval));
        assert!(!retry_allowed(
            Some(attempt),
            at("2026-08-29T09:40:00Z"),
            interval
        ));
        assert!(retry_allowed(
            Some(attempt),
            at("2026-08-29T09:45:00Z"),
            interval
        ));
    }

    #[test]
    fn retention_keeps_at_most_the_configured_number_of_snapshots() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");

        for day in 1..=10 {
            create_snapshot(
                &store.backups,
                &store.sources(),
                SnapshotKind::Automatic,
                at(&format!("2026-08-{day:02}T09:30:00Z")),
                SNAPSHOT_RETENTION,
            )
            .expect("snapshot");
        }

        let snapshots = list_snapshots(&store.backups);
        assert_eq!(snapshots.len(), SNAPSHOT_RETENTION);
        assert_eq!(snapshots[0].name, "2026-08-04T09-30-00Z");
        assert_eq!(
            snapshots[SNAPSHOT_RETENTION - 1].name,
            "2026-08-10T09-30-00Z"
        );
    }

    #[test]
    fn retention_happens_only_after_a_successful_new_backup() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        for day in 1..=3 {
            create_snapshot(
                &store.backups,
                &store.sources(),
                SnapshotKind::Automatic,
                at(&format!("2026-08-{day:02}T09:30:00Z")),
                2,
            )
            .expect("snapshot");
        }
        let kept = list_snapshots(&store.backups);
        assert_eq!(kept.len(), 2);

        // Now break the store so the next backup cannot be built at all.
        fs::remove_dir_all(&store.trash).expect("remove trash");
        fs::write(&store.trash, "não é um diretório").expect("occupy");

        create_snapshot(
            &store.backups,
            &store.sources(),
            SnapshotKind::Automatic,
            at("2026-08-04T09:30:00Z"),
            1,
        )
        .expect_err("the backup must fail");

        // The retention that would have kept only one never ran: a backup that
        // failed must never be paid for with the protection already on disk.
        assert_eq!(list_snapshots(&store.backups), kept);
    }

    #[test]
    fn a_snapshot_that_cannot_be_pruned_does_not_fail_the_backup() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        for day in 1..=3 {
            create_snapshot(
                &store.backups,
                &store.sources(),
                SnapshotKind::Automatic,
                at(&format!("2026-08-{day:02}T09:30:00Z")),
                SNAPSHOT_RETENTION,
            )
            .expect("snapshot");
        }

        let failures = apply_retention_with(&store.backups, 1, |_| {
            Err(std::io::Error::other("simulated removal failure"))
        });
        assert_eq!(failures.len(), 2);
        // Nothing was removed, and every snapshot is still a valid one.
        assert_eq!(list_snapshots(&store.backups).len(), 3);
    }

    #[test]
    fn the_scratch_sweep_removes_only_its_own_leftovers() {
        let store = store();
        fs::create_dir_all(&store.backups).expect("backups");
        fs::create_dir_all(store.backups.join(".tmp.1.1")).expect("scratch");
        fs::create_dir_all(store.backups.join("2026-08-29T09-30-00Z")).expect("snapshot-shaped");
        fs::write(store.backups.join("LEIA-ME.txt"), "arquivo do usuário").expect("user file");
        fs::write(store.backups.join(".tmp.nao-e-diretorio"), "arquivo").expect("temp-named file");

        remove_stale_temp_directories(&store.backups);

        assert!(!store.backups.join(".tmp.1.1").exists());
        assert!(store.backups.join("2026-08-29T09-30-00Z").exists());
        assert!(store.backups.join("LEIA-ME.txt").exists());
        assert!(
            store.backups.join(".tmp.nao-e-diretorio").exists(),
            "only directories are swept"
        );
    }

    #[test]
    fn a_snapshot_round_trips_into_a_fresh_isolated_store() {
        // Phase 3.9, section 5: the snapshot has to be provably restorable, and
        // never onto the real store. A second, empty tree stands in for the
        // machine a recovery would happen on.
        let store = store();
        let note_id = uuid::Uuid::new_v4();
        let trashed_id = uuid::Uuid::new_v4();
        let note = format!(
            "---\nnote_it:\n  version: 1\n  id: {note_id}\n  color: blue\n  \
             paper_type: dotted\n  paper_intensity: subtle\n  font_size: 15\n  \
             created_at: 2026-01-01T00:00:00Z\n  updated_at: 2026-02-02T10:11:12Z\n---\n\n\
             MARCADOR-RESTAURO\n\n- [x] tarefa\n"
        );
        fs::write(store.notes.join(format!("{note_id}.md")), &note).expect("note");
        fs::write(
            store.trash.join(format!("{trashed_id}.md")),
            "nota na lixeira\n",
        )
        .expect("trashed note");
        fs::write(&store.config, "theme = \"dark\"\n").expect("config");
        fs::write(&store.state, "{\"active_layer_mode\":\"desktop\"}").expect("state");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        // The documented manual recovery: copy the snapshot's four parts into
        // an empty tree, with the application closed.
        let recovery = tempdir().expect("recovery tree");
        let recovered_notes = recovery.path().join("data/note-it/notes");
        let recovered_trash = recovery.path().join("data/note-it/trash");
        let recovered_config = recovery.path().join("config/note-it/config.toml");
        let recovered_state = recovery.path().join("state/note-it/state.json");
        fs::create_dir_all(&recovered_notes).expect("notes");
        fs::create_dir_all(&recovered_trash).expect("trash");
        fs::create_dir_all(recovered_config.parent().unwrap()).expect("config dir");
        fs::create_dir_all(recovered_state.parent().unwrap()).expect("state dir");
        for (from, to) in [
            (snapshot.join("notes"), recovered_notes.clone()),
            (snapshot.join("trash"), recovered_trash.clone()),
        ] {
            for entry in fs::read_dir(from).expect("read").flatten() {
                fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
            }
        }
        fs::copy(snapshot.join("config.toml"), &recovered_config).expect("config");
        fs::copy(snapshot.join("state.json"), &recovered_state).expect("state");

        // And the application reads the result.
        let manager = crate::storage::StorageManager::with_custom_paths(
            recovered_notes,
            recovered_config.parent().unwrap().to_path_buf(),
            recovered_state.parent().unwrap().to_path_buf(),
            recovery.path().join("runtime"),
        )
        .expect("open the recovered store");

        let listed = manager.list_notes_by_recency().expect("list");
        assert_eq!(listed, vec![note_id]);
        let loaded = manager
            .load_note(&note_id)
            .expect("load the recovered note");
        assert_eq!(loaded.metadata.id, note_id);
        assert_eq!(loaded.metadata.color, "blue");
        assert_eq!(loaded.metadata.paper_type, "dotted");
        assert_eq!(
            loaded.metadata.updated_at,
            Some(at("2026-02-02T10:11:12Z")),
            "a restored note keeps the date it was last written in"
        );
        assert!(loaded.content.contains("MARCADOR-RESTAURO"));
        assert!(loaded.content.contains("- [x] tarefa"));

        // Byte for byte, in fact.
        assert_eq!(
            fs::read_to_string(manager.note_path(&note_id)).expect("raw"),
            note
        );

        let trash = crate::trash::list_trash(manager.trash_dir());
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].note_id, trashed_id);

        assert_eq!(
            crate::settings::AppConfig::load_from_file(&manager.config_file_path()).theme,
            "dark"
        );
        assert_eq!(
            crate::state::AppState::load_from_file(&manager.state_file_path()).active_layer_mode,
            crate::state::LayerMode::Desktop
        );
    }

    #[test]
    fn a_backups_directory_that_cannot_be_created_fails_the_backup_and_nothing_else() {
        // Reliability audit, case E. A file where the backups directory
        // belongs: `create_dir_all` fails on path resolution, for every user.
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        fs::write(&store.backups, "não é um diretório").expect("occupy the backups path");

        let error = store
            .backup(at("2026-08-29T09:30:00Z"))
            .expect_err("there is nowhere to write a snapshot");
        assert!(error.contains("backups directory"), "{error}");

        assert!(list_snapshots(&store.backups).is_empty());
        // The store itself is untouched: a backup never edits what it copies.
        assert_eq!(
            fs::read_to_string(store.notes.join("a.md")).expect("read"),
            "nota"
        );
    }
}
