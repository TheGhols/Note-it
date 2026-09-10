//! External-editor files are private working copies, never store paths.
//! Keep the last copy on every error; deletion is explicit, never in Drop.
use crate::document::LoadedDocument;
use noteit_core::{
    authority,
    model::NoteDocument,
    write::{NoteMutation, WriteError, WriteOperation},
    StorePaths, Uuid,
};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

/// Preserve EDITOR as a program name (including spaces); arguments can be put
/// in a wrapper script. Do not interpret editor configuration through a shell.
pub fn editor_program(configured: Option<OsString>) -> OsString {
    configured
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "vi".into())
}

/// The single name under the store's state directory where refused text goes.
pub const RECOVERY_DIRECTORY: &str = "tui-recovery";

/// Writes `bytes` where they can be found again, and returns where.
///
/// One implementation for both editors: the external one preserves what a
/// `$EDITOR` returned, the native one preserves the draft still on screen, and
/// both must land in the same place with the same guarantees — `0600` file,
/// `0700` directories, a fresh UUID so nothing existing is overwritten, and
/// file *and* directory synchronized before the caller is told it worked.
/// These bytes are exactly what was refused: no front matter, no serialization,
/// no canonicalization, and a zero-byte file when the text was emptied.
pub fn preserve_draft(directory: &Path, note_id: Uuid, bytes: &[u8]) -> io::Result<PathBuf> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)?;
    let path = directory.join(format!("{note_id}-{}.md", Uuid::new_v4()));
    let mut file = private_file(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(directory)?.sync_all()?;
    Ok(path)
}

pub fn recovery_directory(state: Option<OsString>, home: Option<OsString>) -> io::Result<PathBuf> {
    let base = match state.filter(|s| !s.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => {
            PathBuf::from(home.ok_or_else(|| io::Error::other("HOME e XDG_STATE_HOME ausentes"))?)
                .join(".local/state")
        }
    };
    if !base.is_absolute() {
        return Err(io::Error::other("Diretório de recovery deve ser absoluto"));
    }
    Ok(base.join("note-it").join(RECOVERY_DIRECTORY))
}

fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub struct EditorSession {
    pub original: LoadedDocument,
    pub temporary: PathBuf,
}

#[derive(Debug)]
pub struct EditorResult {
    pub message: String,
    pub reload: bool,
    pub conflict: bool,
    pub recovery: Option<PathBuf>,
}

/// Decide before calling the Core; trailing spaces remain significant.
pub fn mutation_for(original: &str, edited: &str) -> Option<NoteMutation> {
    if original == edited
        || NoteDocument::canonical_content(original) == NoteDocument::canonical_content(edited)
    {
        None
    } else if NoteDocument::canonical_content(edited).is_empty() {
        Some(NoteMutation::ClearBody)
    } else {
        Some(NoteMutation::ReplaceBody {
            body: edited.to_owned(),
        })
    }
}

impl EditorSession {
    pub fn prepare(original: LoadedDocument, temporary_directory: &Path) -> io::Result<Self> {
        let temporary =
            temporary_directory.join(format!("noteit-{}-{}.md", original.id, Uuid::new_v4()));
        let mut file = private_file(&temporary)?;
        if let Err(error) = file
            .write_all(original.content.as_bytes())
            .and_then(|()| file.sync_all())
        {
            let _ = fs::remove_file(&temporary); // Only an incomplete copy of the unchanged original.
            return Err(error);
        }
        Ok(Self {
            original,
            temporary,
        })
    }

    pub fn run(&self, program: &OsStr, interrupted: &AtomicBool) -> io::Result<ExitStatus> {
        let mut child = Command::new(program).arg(&self.temporary).spawn()?;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            }
            if interrupted.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Editor interrompido",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn preserved(&self, message: impl std::fmt::Display) -> EditorResult {
        EditorResult {
            message: format!(
                "{message}. Cópia preservada em {}",
                self.temporary.display()
            ),
            reload: false,
            conflict: false,
            recovery: None,
        }
    }

    fn cleanup(&self, result: &mut EditorResult) {
        if let Err(error) = fs::remove_file(&self.temporary) {
            result.message.push_str(&format!(
                "; temporário mantido em {}: {error}",
                self.temporary.display()
            ));
        }
    }

    /// Terminal must already have resumed. Raw bytes survive separately from
    /// the UTF-8/canonical view used for selecting the domain mutation.
    pub fn finish(
        &self,
        paths: &StorePaths,
        editor_status: io::Result<ExitStatus>,
        recovery_dir: io::Result<PathBuf>,
    ) -> EditorResult {
        let bytes = match fs::read(&self.temporary) {
            Ok(bytes) => bytes,
            Err(error) => {
                return EditorResult {
                    message: format!("Não foi possível ler a saída do editor: {error}. A TUI não removeu o temporário: {}", self.temporary.display()),
                    reload: false,
                    conflict: false,
                    recovery: None,
                };
            }
        };
        let edited = match std::str::from_utf8(&bytes) {
            Ok(edited) => edited,
            Err(error) => {
                return self.preserved(format!("Editor produziu Markdown não UTF-8: {error}"))
            }
        };
        let Some(mutation) = mutation_for(&self.original.content, edited) else {
            let message = match editor_status {
                Ok(status) if status.success() => {
                    "Sem alteração canônica; nenhuma escrita".to_owned()
                }
                status => {
                    format!("Editor não concluiu normalmente ({status:?}); sem alteração canônica")
                }
            };
            let mut result = EditorResult {
                message,
                reload: false,
                conflict: false,
                recovery: None,
            };
            self.cleanup(&mut result);
            return result;
        };
        match editor_status {
            Ok(status) if status.success() => {}
            status => {
                return self.preserved(format!(
                    "Editor não concluiu normalmente ({status:?}); nenhuma escrita"
                ))
            }
        }
        // No reread, no retry. This is the revision captured BEFORE the editor.
        let operation = WriteOperation::MutateNote {
            selector: self.original.id.to_string(),
            mutation,
            expected_revision: Some(self.original.revision.clone()),
        };
        match authority::perform_at(paths, &operation) {
            Ok(_) => {
                let mut result = EditorResult {
                    message: "Edição salva".into(),
                    reload: true,
                    conflict: false,
                    recovery: None,
                };
                self.cleanup(&mut result);
                result
            }
            Err(error @ WriteError::RevisionConflict { .. }) => {
                let recovery = recovery_dir.and_then(|directory| self.recover(&directory, &bytes));
                match recovery {
                    Ok(path) => {
                        let mut result = EditorResult {
                            message: format!(
                                "Conflito de revision; nenhuma sobrescrita. Recovery: {}",
                                path.display()
                            ),
                            reload: true,
                            conflict: true,
                            recovery: Some(path),
                        };
                        self.cleanup(&mut result);
                        result
                    }
                    Err(recovery_error) => {
                        let mut result = self.preserved(format!(
                            "Conflito de revision ({error}); falha no recovery: {recovery_error}"
                        ));
                        result.reload = true;
                        result.conflict = true;
                        result
                    }
                }
            }
            Err(error) => self.preserved(format!("Edição não confirmada: {error}")),
        }
    }

    /// The original temporary is not deleted until both the contents and the
    /// recovery directory entry are synchronized successfully.
    fn recover(&self, directory: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        preserve_draft(directory, self.original.id, bytes)
    }
}
