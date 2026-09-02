//! Local snapshots of everything that can be recovered.
//!
//! A backup is a plain directory of plain files: the notes, the trash, the
//! images those notes hold, the configuration and the window state, copied as
//! they are. No archive, no database, no format of its own. Whatever goes
//! wrong with Note-it, a snapshot can be read with `ls` and put back with
//! `cp`, and that is the whole point of it.
//!
//! **Everything recoverable, or it is not a backup.** A note that says
//! `![](../assets/…)` is only half a note without the file that reference
//! points at, so `assets/` is copied with the same guarantees as the notes
//! themselves and a snapshot that could not copy one is not committed at all.
//! Phase 3.12 introduced those files and this did not learn about them until
//! 3.12R; a snapshot taken in between restores the Markdown and not the
//! pictures. See ADR-032.
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

/// The manifest format a snapshot taken now is written in.
///
/// Version 3 is version 2 plus optional `study.json`. The number is what a snapshot says
/// about *itself*, so a directory written before images existed keeps saying
/// version 1 and stays exactly as valid as it was: nothing here branches on
/// the version, and the field defaults, so an older manifest reads back as the
/// zero assets it genuinely had.
pub const MANIFEST_VERSION: u32 = 3;

/// What a snapshot says about itself. Its presence is also what makes the
/// directory a snapshot rather than a directory someone happened to create.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotManifest {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub kind: String,
    pub notes: usize,
    pub trash: usize,
    /// How many image files were copied — files, never directories. Absent
    /// from a manifest written before 3.12R, where it reads as the nought it
    /// truthfully was.
    #[serde(default)]
    pub assets: usize,
    pub config: bool,
    pub state: bool,
    /// Whether durable review metadata was present. Absent from v1/v2.
    #[serde(default)]
    pub study: bool,
}

/// One committed snapshot on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub path: PathBuf,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// The recoverable things a snapshot copies. Everything else in the data directory —
/// the backups themselves above all — is deliberately not here.
#[derive(Debug, Clone)]
pub struct BackupSources {
    pub notes_dir: PathBuf,
    pub trash_dir: PathBuf,
    /// The images the notes hold, as a tree of `<note-uuid>/<asset-uuid>.<ext>`.
    pub assets_dir: PathBuf,
    pub config_file: PathBuf,
    pub state_file: PathBuf,
    pub study_file: PathBuf,
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
    crate::permissions::create_private_dir_all(backups_dir)
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
    crate::permissions::create_private_dir_all(temp)
        .map_err(|e| format!("Failed to create the scratch directory for the snapshot: {e}"))?;

    let notes = copy_directory(&sources.notes_dir, &temp.join("notes"))?;
    let trash = copy_directory(&sources.trash_dir, &temp.join("trash"))?;
    let assets = copy_assets_tree(&sources.assets_dir, &temp.join("assets"))?;
    let config = copy_optional_file(&sources.config_file, &temp.join("config.toml"))?;
    let state = copy_optional_file(&sources.state_file, &temp.join("state.json"))?;
    let study = copy_optional_file_strict(&sources.study_file, &temp.join("study.json"))?;

    // Written only once every copy above has succeeded, so a manifest can
    // never claim a file the snapshot does not hold.
    let manifest = SnapshotManifest {
        version: MANIFEST_VERSION,
        created_at: now,
        kind: kind.as_str().to_string(),
        notes,
        trash,
        assets,
        config,
        state,
        study,
    };
    let serialized = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("Failed to serialize the snapshot manifest: {e}"))?;
    crate::permissions::write_private_file(&temp.join(MANIFEST_FILE), serialized.as_bytes())
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
    crate::permissions::create_private_dir_all(destination)?;

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

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                eprintln!(
                    "Skipping unreadable entry in {}: {error}",
                    path.display()
                );
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            eprintln!(
                "Skipping symbolic link in {}: backups do not follow links",
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

/// Copies the images the notes hold, keeping the shape they are stored in.
///
/// `assets/` is not a flat directory, so [`copy_directory`] cannot serve it:
/// it is one directory per note, holding one file per picture. This walks
/// exactly those two levels and no more. It is deliberately not a general
/// recursive copy — a routine that descends wherever it finds a directory is
/// how a backup ends up following something out of the tree it was asked to
/// copy, and there is nothing in a correct `assets/` for it to find down there.
///
/// **Strict, and fail-closed**, which is where this parts company with the
/// notes. `notes/` holds files a person may reasonably have put there
/// themselves, so an oddity is skipped with a warning. `assets/` is written by
/// Note-it and by nothing else, so an oddity means the store is not in the
/// state this believes it to be — and quietly omitting managed content while
/// reporting a complete backup is the one failure a backup may never have.
/// Anything that is not the expected shape stops the snapshot before it is
/// committed.
///
/// What *is* expected, and skipped rather than refused, is scratch: an
/// interrupted import leaves a `.tmp.…` beside the file it was writing, the
/// same way an interrupted note save does. A name beginning with `.` is never
/// committed content and never part of a snapshot.
///
/// Nothing decides here whether a picture is still *used*. An asset no note
/// points at any more is managed content and is copied like the rest: Phase
/// 3.12 chose not to collect orphans, and a backup is not the place to start
/// doing it by omission.
///
/// Returns the number of image files copied.
fn copy_assets_tree(source: &Path, destination: &Path) -> Result<usize, String> {
    crate::permissions::create_private_dir_all(destination)?;

    // A store written before images existed has no assets directory at all,
    // and that is a store with no pictures rather than a broken one.
    let Ok(metadata) = fs::symlink_metadata(source) else {
        return Ok(0);
    };
    if !metadata.file_type().is_dir() {
        return Err(format!(
            "Failed to back up the managed images: {} is not a directory",
            source.display()
        ));
    }

    let entries = fs::read_dir(source)
        .map_err(|e| format!("Failed to read {} for the backup: {e}", source.display()))?;

    let mut copied = 0;
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("Failed to read an entry of {}: {e}", source.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(format!(
                "Failed to back up {}: an entry has a name that is not text",
                source.display()
            ));
        };

        if name.starts_with('.') {
            continue;
        }

        let metadata = fs::symlink_metadata(&path).map_err(|e| {
            format!(
                "Failed to inspect the managed images at {}: {e}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Failed to back up the managed images: {} is a symbolic link",
                path.display()
            ));
        }
        if !metadata.file_type().is_dir() {
            return Err(format!(
                "Failed to back up the managed images: {} is not a directory",
                path.display()
            ));
        }

        // Must be a valid note UUID, matching parse_asset_request's rule.
        if uuid::Uuid::parse_str(name).is_err() {
            return Err(format!(
                "Failed to back up the managed images: {} is not a note identifier",
                path.display()
            ));
        }

        copied += copy_note_assets(&path, &destination.join(name), name)?;
    }

    Ok(copied)
}

/// Copies the images belonging to one note.
///
/// Each name is validated by [`crate::assets::parse_asset_request`] — the same
/// function that decides what the page is allowed to ask the host for — so a
/// snapshot holds exactly the files the application can serve, and the two can
/// never come to disagree about what a managed asset is.
///
/// The file keeps the name it has on disk. Nothing is renamed into the
/// canonical spelling on the way into a snapshot, because a note's reference
/// names the file that exists and a backup that "tidied" it would restore a
/// broken link.
fn copy_note_assets(source: &Path, destination: &Path, note: &str) -> Result<usize, String> {
    crate::permissions::create_private_dir_all(destination)?;

    let entries = fs::read_dir(source)
        .map_err(|e| format!("Failed to read {} for the backup: {e}", source.display()))?;

    let mut copied = 0;
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("Failed to read an entry of {}: {e}", source.display()))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(format!(
                "Failed to back up the managed images: {} has a name that is not text",
                path.display()
            ));
        };
        if name.starts_with('.') {
            continue;
        }

        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Failed to back up the managed images: {} is a symbolic link, \
                 and a backup never follows one",
                path.display()
            ));
        }
        if !metadata.file_type().is_file() {
            return Err(format!(
                "Failed to back up the managed images: {} is not an image file",
                path.display()
            ));
        }
        if crate::assets::parse_asset_request(&format!("/{note}/{name}")).is_none() {
            return Err(format!(
                "Failed to back up the managed images: {} is not a managed asset",
                path.display()
            ));
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

/// Study history is managed recoverable data. If it exists, anything other
/// than a readable regular file fails the snapshot rather than silently
/// producing a backup that claims to be complete without it.
fn copy_optional_file_strict(source: &Path, destination: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Failed to inspect {} for the backup: {error}",
                source.display()
            ))
        }
    };
    if !metadata.file_type().is_file() {
        return Err(format!(
            "Failed to back up study history: {} is not a regular file",
            source.display()
        ));
    }
    fs::copy(source, destination).map_err(|error| {
        format!(
            "Failed to copy {} into the snapshot: {error}",
            source.display()
        )
    })?;
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
        assets: PathBuf,
        config: PathBuf,
        state: PathBuf,
        study: PathBuf,
        backups: PathBuf,
    }

    impl Store {
        fn sources(&self) -> BackupSources {
            BackupSources {
                notes_dir: self.notes.clone(),
                trash_dir: self.trash.clone(),
                assets_dir: self.assets.clone(),
                config_file: self.config.clone(),
                state_file: self.state.clone(),
                study_file: self.study.clone(),
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
        let assets = root.join("assets");
        let backups = root.join("backups");
        fs::create_dir_all(&notes).expect("notes");
        fs::create_dir_all(&trash).expect("trash");
        fs::create_dir_all(&assets).expect("assets");
        Store {
            _tmp: tmp,
            config: root.join("config.toml"),
            state: root.join("state.json"),
            study: root.join("study.json"),
            root,
            notes,
            trash,
            assets,
            backups,
        }
    }

    /// A store as it was before images existed: no `assets/` at all.
    fn store_without_assets() -> Store {
        let store = store();
        fs::remove_dir_all(&store.assets).expect("remove the assets directory");
        store
    }

    /// Bytes that are a real PNG as far as the asset subsystem is concerned.
    fn png(seed: u8) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&[seed; 64]);
        bytes
    }

    /// Writes one managed image and returns its bytes.
    fn put_asset(store: &Store, note: &str, asset: &str, extension: &str, seed: u8) -> Vec<u8> {
        let directory = store.assets.join(note);
        fs::create_dir_all(&directory).expect("note asset directory");
        let bytes = png(seed);
        fs::write(directory.join(format!("{asset}.{extension}")), &bytes).expect("asset");
        bytes
    }

    fn note_uuid(n: u8) -> String {
        format!("{n:08x}-1111-4111-8111-111111111111")
    }

    fn asset_uuid(n: u8) -> String {
        format!("{n:08x}-2222-4222-8222-222222222222")
    }

    fn manifest_of(snapshot: &Path) -> SnapshotManifest {
        read_manifest(&snapshot.join(MANIFEST_FILE)).expect("a snapshot has a manifest")
    }

    /// Copies a snapshot subtree the way a person restoring one with `cp -r`
    /// would. Test scaffolding: the application never does this.
    fn copy_tree_for_test(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).expect("destination");
        let Ok(entries) = fs::read_dir(source) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().expect("name").to_owned();
            if path.is_dir() {
                copy_tree_for_test(&path, &destination.join(name));
            } else {
                fs::copy(&path, destination.join(name)).expect("copy");
            }
        }
    }

    fn digest(path: &Path) -> Vec<u8> {
        // Comparing the bytes themselves rather than a hash of them: this is
        // the strongest statement available and needs no dependency.
        fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
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
        assert!(!manifest.study);
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
        assert!(!manifest.study);
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
        let review_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let study = format!(
            "{{\n  \"version\": 1,\n  \"algorithm\": \"ladder-v1\",\n  \"cards\": {{\n    \"{review_key}\": {{\n      \"level\": 2,\n      \"due_at\": \"2026-09-02T10:00:00Z\",\n      \"last_reviewed_at\": \"2026-08-30T10:00:00Z\",\n      \"review_count\": 1,\n      \"last_rating\": \"easy\"\n    }}\n  }},\n  \"days\": {{\n    \"2026-08-30\": {{\"reviews\": 1, \"difficult\": 0, \"medium\": 0, \"easy\": 1}}\n  }}\n}}\n"
        );
        fs::write(&store.study, &study).expect("study history");

        let snapshot = store.backup(at("2026-08-29T09:30:00Z")).expect("backup");

        // The documented manual recovery: copy the snapshot's four parts into
        // an empty tree, with the application closed.
        let recovery = tempdir().expect("recovery tree");
        let recovered_notes = recovery.path().join("data/note-it/notes");
        let recovered_trash = recovery.path().join("data/note-it/trash");
        let recovered_config = recovery.path().join("config/note-it/config.toml");
        let recovered_state = recovery.path().join("state/note-it/state.json");
        let recovered_study = recovery.path().join("data/note-it/study.json");
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
        fs::copy(snapshot.join("study.json"), &recovered_study).expect("study");

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
        let recovered_study_state = manager.load_study().expect("study state");
        assert_eq!(recovered_study_state.cards[review_key].level, 2);
        assert_eq!(
            recovered_study_state.days[&chrono::NaiveDate::from_ymd_opt(2026, 8, 30).unwrap()].easy,
            1
        );
        assert_eq!(
            fs::read_to_string(manager.study_file_path()).expect("raw study"),
            study
        );
        assert!(manifest_of(&snapshot).study);
    }

    #[test]
    fn study_history_is_optional_but_never_silently_omitted_when_present() {
        let absent = store();
        fs::write(absent.notes.join("a.md"), "nota").expect("note");
        let snapshot = absent.backup(at("2026-08-30T09:30:00Z")).expect("backup");
        assert!(!snapshot.join("study.json").exists());
        assert!(!manifest_of(&snapshot).study);

        let malformed = store();
        fs::write(malformed.notes.join("a.md"), "nota").expect("note");
        fs::create_dir(&malformed.study).expect("study path occupied by a directory");
        let error = malformed
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("an incomplete study backup must not commit");
        assert!(error.contains("study history"), "{error}");
        assert!(list_snapshots(&malformed.backups).is_empty());
    }

    // ---------------------------------------------------------------- assets
    //
    // Phase 3.12 put a note's images in `assets/<note>/<asset>.<ext>` and this
    // routine did not know about them, so a snapshot restored the Markdown and
    // not the pictures it pointed at. Everything below is that gap closed.

    #[test]
    fn a_store_with_no_images_still_backs_up_and_says_so() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota sem imagem").expect("note");

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        // The directory is there and empty, so a restore is a copy of the same
        // five things whether or not the store ever held a picture.
        assert!(snapshot.join("assets").is_dir());
        assert!(names_in(&snapshot.join("assets")).is_empty());
        assert_eq!(manifest_of(&snapshot).assets, 0);
    }

    #[test]
    fn a_store_written_before_images_existed_backs_up_unchanged() {
        // No `assets/` at all, which is every store that predates Phase 3.12.
        // That is a store with no pictures, not a broken one.
        let store = store_without_assets();
        fs::write(store.notes.join("a.md"), "nota antiga").expect("note");
        assert!(!store.assets.exists());

        let snapshot = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect("a store from before images still backs up");

        assert_eq!(manifest_of(&snapshot).assets, 0);
        assert_eq!(
            fs::read_to_string(snapshot.join("notes/a.md")).expect("a"),
            "nota antiga"
        );
        // And the store is not given a directory it did not have.
        assert!(!store.assets.exists());
    }

    #[test]
    fn one_image_travels_with_the_note_that_points_at_it() {
        let store = store();
        let note = note_uuid(1);
        let asset = asset_uuid(1);
        fs::write(
            store.notes.join(format!("{note}.md")),
            format!("![](../assets/{note}/{asset}.png)"),
        )
        .expect("note");
        let bytes = put_asset(&store, &note, &asset, "png", 0xA1);

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        let copied = snapshot
            .join("assets")
            .join(&note)
            .join(format!("{asset}.png"));
        assert!(copied.is_file(), "the image did not travel with the note");
        // Byte for byte. A backup copies bytes and does nothing else to them:
        // no recompression, no conversion, no metadata rewritten.
        assert_eq!(digest(&copied), bytes);
        assert_eq!(manifest_of(&snapshot).assets, 1);
        // The note is not rewritten, so the reference still resolves.
        assert_eq!(
            fs::read_to_string(snapshot.join("notes").join(format!("{note}.md"))).expect("note"),
            format!("![](../assets/{note}/{asset}.png)")
        );
    }

    #[test]
    fn every_image_of_every_note_travels() {
        // Two notes, three pictures, and one of the notes holds two of them.
        let store = store();
        let (first, second) = (note_uuid(1), note_uuid(2));
        let bytes = [
            put_asset(&store, &first, &asset_uuid(1), "png", 0x11),
            put_asset(&store, &first, &asset_uuid(2), "webp", 0x22),
            put_asset(&store, &second, &asset_uuid(3), "jpg", 0x33),
        ];

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");
        let assets = snapshot.join("assets");

        assert_eq!(manifest_of(&snapshot).assets, 3);
        assert_eq!(names_in(&assets), vec![first.clone(), second.clone()]);
        assert_eq!(
            names_in(&assets.join(&first)),
            vec![
                format!("{}.png", asset_uuid(1)),
                format!("{}.webp", asset_uuid(2)),
            ]
        );
        assert_eq!(
            names_in(&assets.join(&second)),
            vec![format!("{}.jpg", asset_uuid(3))]
        );

        // The shape is kept: one directory per note, never flattened, and the
        // bytes of each are the bytes that were stored.
        assert_eq!(
            digest(&assets.join(&first).join(format!("{}.png", asset_uuid(1)))),
            bytes[0]
        );
        assert_eq!(
            digest(&assets.join(&first).join(format!("{}.webp", asset_uuid(2)))),
            bytes[1]
        );
        assert_eq!(
            digest(&assets.join(&second).join(format!("{}.jpg", asset_uuid(3)))),
            bytes[2]
        );
    }

    #[test]
    fn an_image_no_note_points_at_any_more_is_still_backed_up() {
        // Phase 3.12 chose not to collect orphans, so an unreferenced picture
        // is managed content like any other. A backup that quietly left it out
        // would be collecting them by omission — and deciding a file is
        // unwanted is not a backup's decision to make.
        let store = store();
        let note = note_uuid(1);
        fs::write(
            store.notes.join(format!("{note}.md")),
            "nota sem imagem alguma",
        )
        .expect("note");
        let bytes = put_asset(&store, &note, &asset_uuid(9), "png", 0x99);

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        assert_eq!(manifest_of(&snapshot).assets, 1);
        assert_eq!(
            digest(
                &snapshot
                    .join("assets")
                    .join(&note)
                    .join(format!("{}.png", asset_uuid(9)))
            ),
            bytes
        );
    }

    #[test]
    fn scratch_left_by_an_interrupted_import_is_not_snapshot_content() {
        // An import writes through a temp file in the same directory, exactly
        // as a note save does. What a crash leaves behind was never committed
        // content and is not part of a snapshot — and it does not fail one.
        let store = store();
        let note = note_uuid(1);
        put_asset(&store, &note, &asset_uuid(1), "png", 0x44);
        fs::write(
            store.assets.join(&note).join(".tmp.partial.png.4242"),
            png(0),
        )
        .expect("scratch beside the asset");
        fs::create_dir_all(store.assets.join(".tmp.4242")).expect("scratch beside the note");

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        assert_eq!(manifest_of(&snapshot).assets, 1);
        assert_eq!(
            names_in(&snapshot.join("assets").join(&note)),
            vec![format!("{}.png", asset_uuid(1))]
        );
        assert_eq!(names_in(&snapshot.join("assets")), vec![note]);
    }

    #[test]
    fn an_assets_path_that_is_not_a_directory_fails_the_backup() {
        // "0 images copied, backup complete" over a store whose managed area
        // is not what it should be would be a snapshot that looks whole and is
        // not. Fail closed.
        let store = store_without_assets();
        fs::write(&store.assets, "não é um diretório").expect("occupy the assets path");
        fs::write(store.notes.join("a.md"), "nota").expect("note");

        let error = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("a broken managed area must not produce a snapshot");
        assert!(error.contains("not a directory"), "{error}");
        assert!(list_snapshots(&store.backups).is_empty());
    }

    #[test]
    fn a_symbolic_link_where_a_notes_images_belong_fails_the_backup() {
        // `assets/<note> -> ~/Pictures`. Following it would copy somebody's
        // photographs into a Note-it snapshot; ignoring it would report a
        // complete backup of a store this no longer understands.
        let store = store();
        let outside = store.root.join("fora");
        fs::create_dir_all(&outside).expect("outside");
        fs::write(outside.join("segredo.png"), png(0xFF)).expect("secret");
        symlink(&outside, store.assets.join(note_uuid(1))).expect("directory symlink");

        let error = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("a symbolic link in the managed area must fail the backup");
        assert!(error.contains("symbolic link"), "{error}");
        assert!(list_snapshots(&store.backups).is_empty());
        // Nothing from outside the tree was copied, and nothing was left behind.
        assert!(names_in(&store.backups).is_empty());
    }

    #[test]
    fn a_symbolic_link_where_an_image_belongs_fails_the_backup() {
        let store = store();
        let note = note_uuid(1);
        fs::create_dir_all(store.assets.join(&note)).expect("note asset directory");
        let outside = store.root.join("passwd");
        fs::write(&outside, "root:x:0:0").expect("outside file");
        symlink(
            &outside,
            store
                .assets
                .join(&note)
                .join(format!("{}.png", asset_uuid(1))),
        )
        .expect("file symlink");

        let error = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("a symbolic link in the managed area must fail the backup");
        assert!(error.contains("symbolic link"), "{error}");
        assert!(list_snapshots(&store.backups).is_empty());
    }

    #[test]
    fn anything_that_is_not_the_expected_shape_fails_the_backup() {
        // `assets/` is written by Note-it and by nothing else, so an entry
        // this does not recognise means the store is not in the state it is
        // believed to be. Each case is checked in a store of its own, because
        // the first one is meant to stop the backup.
        /// One way a managed area can be wrong, and how to arrange it.
        type MalformedCase = (&'static str, Box<dyn Fn(&Store)>);

        let cases: Vec<MalformedCase> = vec![
            (
                "a directory not named after a note",
                Box::new(|store: &Store| {
                    fs::create_dir_all(store.assets.join("nao-e-uuid")).expect("directory");
                }),
            ),
            (
                "a loose file where a note's directory belongs",
                Box::new(|store: &Store| {
                    fs::write(store.assets.join("solto.png"), png(1)).expect("file");
                }),
            ),
            (
                "a directory inside a note's images",
                Box::new(move |store: &Store| {
                    fs::create_dir_all(store.assets.join(note_uuid(1)).join("subdir"))
                        .expect("nested directory");
                }),
            ),
            (
                "a file that is not a managed asset",
                Box::new(|store: &Store| {
                    let directory = store.assets.join(note_uuid(1));
                    fs::create_dir_all(&directory).expect("directory");
                    fs::write(directory.join("qualquer-coisa.txt.bak"), "lixo").expect("file");
                }),
            ),
            (
                "an asset in a format the store does not hold",
                Box::new(|store: &Store| {
                    let directory = store.assets.join(note_uuid(1));
                    fs::create_dir_all(&directory).expect("directory");
                    fs::write(directory.join(format!("{}.svg", asset_uuid(1))), "<svg/>")
                        .expect("file");
                }),
            ),
            (
                "an asset whose name is not an identifier",
                Box::new(|store: &Store| {
                    let directory = store.assets.join(note_uuid(1));
                    fs::create_dir_all(&directory).expect("directory");
                    fs::write(directory.join("bad-name.png"), png(1)).expect("file");
                }),
            ),
        ];

        for (description, arrange) in cases {
            let store = store();
            fs::write(store.notes.join("a.md"), "nota").expect("note");
            arrange(&store);

            let error = store
                .backup(at("2026-08-30T09:30:00Z"))
                .expect_err(description);
            assert!(error.contains("managed images"), "{description}: {error}");
            assert!(
                list_snapshots(&store.backups).is_empty(),
                "{description}: a snapshot was committed anyway"
            );
        }
    }

    #[test]
    fn an_image_that_cannot_be_read_stops_the_snapshot_before_it_exists() {
        // The whole transaction, exercised through the one thing that can fail
        // late: the notes are copied, and then an image cannot be. Nothing may
        // be committed, nothing swept, and the previous snapshot must survive.
        use std::os::unix::fs::PermissionsExt;

        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        let note = note_uuid(1);
        put_asset(&store, &note, &asset_uuid(1), "png", 0x55);

        // An earlier snapshot, so there is something a failed run could damage.
        let earlier = store
            .backup(at("2026-08-29T09:30:00Z"))
            .expect("first backup");

        let unreadable = store
            .assets
            .join(&note)
            .join(format!("{}.png", asset_uuid(1)));
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
            .expect("make the asset unreadable");

        // A process that bypasses permission bits — root, which is what CI
        // runs as — cannot be given an unreadable file, so the premise of this
        // test does not exist there. The transaction it guards is covered for
        // every user by the run below, which arranges a failure the filesystem
        // enforces rather than one the caller is trusted to respect.
        if fs::read(&unreadable).is_ok() {
            fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644))
                .expect("restore permissions");
            return;
        }

        let error = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("an image that cannot be copied must fail the snapshot");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644))
            .expect("restore permissions");

        assert!(error.contains("into the snapshot"), "{error}");
        // No second snapshot, and no scratch left claiming to be one.
        let snapshots = list_snapshots(&store.backups);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].path, earlier);
        assert!(names_in(&store.backups)
            .iter()
            .all(|name| !name.starts_with(TEMP_PREFIX)));
    }

    #[test]
    fn a_failure_while_copying_the_images_commits_nothing() {
        // The same transaction as above, arranged so it holds for every user:
        // the notes and the trash copy, and then the managed area turns out
        // not to be what it should be. Everything up to the rename can fail
        // with nothing gained and nothing lost.
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        fs::write(store.trash.join("b.md"), "na lixeira").expect("trashed note");
        let earlier = store
            .backup(at("2026-08-29T09:30:00Z"))
            .expect("first backup");

        // One good image, and one entry that cannot be part of a snapshot.
        put_asset(&store, &note_uuid(1), &asset_uuid(1), "png", 0x55);
        fs::create_dir_all(store.assets.join(note_uuid(2)).join("subdir"))
            .expect("an unexpected directory inside a note's images");

        let error = store
            .backup(at("2026-08-30T09:30:00Z"))
            .expect_err("a managed area that is not what it should be must fail the snapshot");

        assert!(error.contains("managed images"), "{error}");
        // Exactly the snapshot that was there before, and no scratch beside it.
        let snapshots = list_snapshots(&store.backups);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].path, earlier);
        assert!(names_in(&store.backups)
            .iter()
            .all(|name| !name.starts_with(TEMP_PREFIX)));
        // The store itself is untouched: a backup never edits what it copies.
        assert_eq!(
            fs::read_to_string(store.notes.join("a.md")).expect("read"),
            "nota"
        );
    }

    #[test]
    fn a_failed_image_copy_never_prunes_an_old_snapshot_to_make_room() {
        // Retention runs after a commit and only after one. A run that fails
        // while copying the images must not have deleted anything on its way
        // there — trading protection already on disk for a backup that then
        // did not happen is the one thing the create-commit-prune order exists
        // to prevent.
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        for day in 1..=3 {
            store
                .backup(at(&format!("2026-08-{day:02}T09:30:00Z")))
                .expect("earlier backup");
        }
        let before = names_in(&store.backups);
        assert_eq!(before.len(), 3);

        // A managed area that cannot be snapshotted, arranged so the filesystem
        // enforces it for every user rather than the caller's permissions.
        fs::create_dir_all(store.assets.join("nao-e-uuid")).expect("a malformed entry");

        // Two kept, three on disk: a successful run here would prune one.
        let error = create_snapshot(
            &store.backups,
            &store.sources(),
            SnapshotKind::Automatic,
            at("2026-08-30T09:30:00Z"),
            2,
        )
        .expect_err("the snapshot must fail");

        assert!(error.contains("managed images"), "{error}");
        assert_eq!(
            names_in(&store.backups),
            before,
            "a failed backup pruned an old one"
        );
    }

    #[test]
    fn a_manifest_written_before_images_existed_is_still_a_snapshot() {
        // Every backup on disk today was written by version 1. None of them
        // may become unreadable, unlistable, or stop counting as the most
        // recent snapshot.
        let store = store();
        fs::create_dir_all(&store.backups).expect("backups");
        let old = store.backups.join("2026-08-01T09-30-00Z");
        fs::create_dir_all(old.join("notes")).expect("old snapshot");
        fs::write(
            old.join(MANIFEST_FILE),
            r#"{
                "version": 1,
                "created_at": "2026-08-01T09:30:00Z",
                "kind": "automatic",
                "notes": 2,
                "trash": 0,
                "config": true,
                "state": true
            }"#,
        )
        .expect("a version 1 manifest");

        let snapshots = list_snapshots(&store.backups);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].created_at, at("2026-08-01T09:30:00Z"));
        assert_eq!(
            last_snapshot_time(&store.backups),
            Some(at("2026-08-01T09:30:00Z"))
        );

        let manifest = manifest_of(&old);
        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.notes, 2);
        // It said nothing about images because it had none to say anything
        // about, and it reads back as exactly that.
        assert_eq!(manifest.assets, 0);
        assert!(!manifest.study);
    }

    #[test]
    fn a_version_two_manifest_without_study_is_still_readable() {
        let store = store();
        fs::create_dir_all(&store.backups).expect("backups");
        let old = store.backups.join("2026-08-30T09-30-00Z");
        fs::create_dir_all(old.join("notes")).expect("old snapshot");
        fs::write(
            old.join(MANIFEST_FILE),
            r#"{
                "version": 2,
                "created_at": "2026-08-30T09:30:00Z",
                "kind": "manual",
                "notes": 1,
                "trash": 0,
                "assets": 2,
                "config": false,
                "state": false
            }"#,
        )
        .expect("a version 2 manifest");

        let manifest = manifest_of(&old);
        assert_eq!(manifest.version, 2);
        assert_eq!(manifest.assets, 2);
        assert!(!manifest.study);
        assert_eq!(list_snapshots(&store.backups).len(), 1);
    }

    #[test]
    fn a_snapshot_taken_now_says_which_version_it_is_and_what_it_holds() {
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        put_asset(&store, &note_uuid(1), &asset_uuid(1), "png", 0x77);
        put_asset(&store, &note_uuid(2), &asset_uuid(2), "gif", 0x88);

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");
        let manifest = manifest_of(&snapshot);

        assert_eq!(manifest.version, MANIFEST_VERSION);
        assert_eq!(manifest.version, 3);
        assert_eq!(manifest.notes, 1);
        assert_eq!(manifest.assets, 2);
        assert!(!manifest.study);

        // The manifest never claims a file the snapshot does not hold: the
        // count is the files on disk under `assets/`.
        let mut on_disk = 0;
        for note in fs::read_dir(snapshot.join("assets")).expect("assets") {
            on_disk += fs::read_dir(note.expect("entry").path())
                .expect("note assets")
                .count();
        }
        assert_eq!(on_disk, manifest.assets);
    }

    #[test]
    fn an_automatic_snapshot_carries_the_images_too() {
        // One routine serves both kinds. There is no "backup with pictures"
        // and "backup without" to fall out of step with each other.
        let store = store();
        fs::write(store.notes.join("a.md"), "nota").expect("note");
        let bytes = put_asset(&store, &note_uuid(1), &asset_uuid(1), "png", 0xAB);

        let snapshot = create_snapshot(
            &store.backups,
            &store.sources(),
            SnapshotKind::Automatic,
            at("2026-08-30T09:30:00Z"),
            SNAPSHOT_RETENTION,
        )
        .expect("automatic backup");

        let manifest = manifest_of(&snapshot);
        assert_eq!(manifest.kind, "automatic");
        assert_eq!(manifest.assets, 1);
        assert_eq!(
            digest(
                &snapshot
                    .join("assets")
                    .join(note_uuid(1))
                    .join(format!("{}.png", asset_uuid(1)))
            ),
            bytes
        );
    }

    #[test]
    fn a_snapshot_restores_a_note_and_its_picture_into_an_empty_store() {
        // The whole point of the phase, end to end: a store, a snapshot, and a
        // second store that gets nothing except what the snapshot holds.
        let store = store();
        let note = note_uuid(1);
        let asset = asset_uuid(1);
        let markdown = format!("# Biópsia\n\n![](../assets/{note}/{asset}.png)\n\nlegenda");
        fs::write(store.notes.join(format!("{note}.md")), &markdown).expect("note");
        let bytes = put_asset(&store, &note, &asset, "png", 0xCD);
        fs::write(&store.config, "theme = \"dark\"\n").expect("config");
        fs::write(&store.state, "{\"notes\":{}}").expect("state");

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        // A second store, and nothing of the first reaches it.
        let restored_tmp = tempdir().expect("tempdir");
        let restored = restored_tmp.path().join("note-it");
        for part in ["notes", "trash", "assets"] {
            copy_tree_for_test(&snapshot.join(part), &restored.join(part));
        }
        fs::create_dir_all(&restored).expect("restored root");
        fs::copy(snapshot.join("config.toml"), restored.join("config.toml")).expect("config");
        fs::copy(snapshot.join("state.json"), restored.join("state.json")).expect("state");

        // The note came back byte for byte, so its reference is the one it had.
        assert_eq!(
            fs::read_to_string(restored.join("notes").join(format!("{note}.md"))).expect("note"),
            markdown
        );
        // ...and the file that reference points at is there, byte for byte.
        let picture = restored
            .join("assets")
            .join(&note)
            .join(format!("{asset}.png"));
        assert_eq!(digest(&picture), bytes);

        // Resolved the way the note resolves it: `../assets/…` from `notes/`.
        let from_note = restored
            .join("notes")
            .join(format!("../assets/{note}/{asset}.png"));
        assert!(
            from_note.exists(),
            "the note's own reference does not resolve"
        );
        // And the host would serve it: the same parse the URI scheme performs.
        let request = crate::assets::parse_asset_request(&format!("/{note}/{asset}.png"))
            .expect("the restored asset is one the application can serve");
        assert_eq!(request.file_path(&restored.join("assets")), picture);
    }

    #[test]
    fn a_note_in_the_trash_keeps_its_picture_through_a_snapshot() {
        // A trashed note is still recoverable content, and its `../assets/…`
        // resolves from `trash/` exactly as it does from `notes/` — which is
        // the reason the reference is relative in the first place.
        let store = store();
        let note = note_uuid(1);
        let asset = asset_uuid(1);
        let markdown = format!("nota descartada\n\n![](../assets/{note}/{asset}.png)");
        fs::write(store.trash.join(format!("{note}.md")), &markdown).expect("trashed note");
        let bytes = put_asset(&store, &note, &asset, "png", 0xEF);

        let snapshot = store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        assert_eq!(manifest_of(&snapshot).trash, 1);
        assert_eq!(manifest_of(&snapshot).assets, 1);

        let restored_tmp = tempdir().expect("tempdir");
        let restored = restored_tmp.path().join("note-it");
        for part in ["notes", "trash", "assets"] {
            copy_tree_for_test(&snapshot.join(part), &restored.join(part));
        }

        // It is still in the trash, and its picture is still there.
        assert_eq!(
            fs::read_to_string(restored.join("trash").join(format!("{note}.md"))).expect("note"),
            markdown
        );
        assert!(restored
            .join("trash")
            .join(format!("../assets/{note}/{asset}.png"))
            .exists());

        // Bringing it back out is the ordinary restore, and the reference is
        // untouched by it: the file moves, its text does not.
        let manager = crate::storage::StorageManager::with_custom_paths(
            restored.join("notes"),
            restored.join("config"),
            restored.join("state"),
            restored.join("runtime"),
        )
        .expect("open the restored store");
        let id = uuid::Uuid::parse_str(&note).expect("note identifier");
        manager
            .restore_note_from_trash(&id)
            .expect("bring the note back");

        assert_eq!(
            fs::read_to_string(manager.note_path(&id)).expect("restored note"),
            markdown
        );
        assert_eq!(
            digest(
                &restored
                    .join("assets")
                    .join(&note)
                    .join(format!("{asset}.png"))
            ),
            bytes
        );
    }

    #[test]
    fn a_backup_never_touches_the_store_it_copies() {
        // Not the notes, not the pictures, not their modification dates. A
        // backup reads.
        let store = store();
        let note = note_uuid(1);
        let asset = asset_uuid(1);
        let markdown = format!("nota\n\n![](../assets/{note}/{asset}.png)");
        let note_path = store.notes.join(format!("{note}.md"));
        fs::write(&note_path, &markdown).expect("note");
        let bytes = put_asset(&store, &note, &asset, "png", 0x12);
        let asset_path = store.assets.join(&note).join(format!("{asset}.png"));

        let before = (
            fs::metadata(&note_path)
                .and_then(|m| m.modified())
                .expect("note mtime"),
            fs::metadata(&asset_path)
                .and_then(|m| m.modified())
                .expect("asset mtime"),
        );

        store.backup(at("2026-08-30T09:30:00Z")).expect("backup");

        assert_eq!(fs::read_to_string(&note_path).expect("note"), markdown);
        assert_eq!(digest(&asset_path), bytes);
        assert_eq!(
            (
                fs::metadata(&note_path)
                    .and_then(|m| m.modified())
                    .expect("note mtime"),
                fs::metadata(&asset_path)
                    .and_then(|m| m.modified())
                    .expect("asset mtime"),
            ),
            before,
            "a backup moved a modification date"
        );
        // Nothing was added to the store either.
        assert_eq!(
            names_in(&store.assets.join(&note)),
            vec![format!("{asset}.png")]
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
