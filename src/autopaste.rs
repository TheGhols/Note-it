//! AutoPaste: the policy that decides whether a clipboard change is a capture.
//!
//! Nothing here touches GDK, a clipboard or a window. What is here is the part
//! that has to be *right* — armed or not, whose note, which generation, which
//! read is stale — and it is a plain state machine so it can be tested without
//! a graphical session and without ever reading anybody's real clipboard.
//! `app.rs` owns the `GdkClipboard` and calls into this; see ADR-031.
//!
//! Three properties are the whole design.
//!
//! **Off means off.** There is no "armed but ignoring". When AutoPaste is off
//! the host has no clipboard handler connected at all, so there is nothing to
//! observe, nothing to read, nothing to hash and nothing to remember. This type
//! never holds clipboard text: it decides, and the text goes straight from the
//! read callback to the target note's WebView.
//!
//! **One target, ever.** The system clipboard is one thing, so a copy cannot
//! sensibly land in two notes. Arming a note disarms whatever was armed before,
//! in the same step that mints the new generation.
//!
//! **A generation, not a comparison.** Every arm and every disarm bumps a
//! counter, and every asynchronous read carries the generation it started
//! under. A read that finishes after the mode was switched off, or after the
//! target changed, is discarded because its generation is gone — not because
//! its text looked familiar. Comparing text would be a different feature with a
//! different bug: two deliberate copies of the same words are two captures.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What goes between the note's existing content and a new capture.
///
/// Persisted in `config.toml` because it is a preference, not a mode: it says
/// how captures should be laid out, and knowing it after a restart tells
/// nobody's secrets. The mode itself is deliberately not persisted at all.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum CaptureDelimiter {
    /// The capture continues on the next line of the same paragraph.
    Line,
    /// The capture becomes its own paragraph. The default.
    #[default]
    BlankLine,
    /// A horizontal rule stands between them.
    Separator,
}

pub const CAPTURE_DELIMITERS: &[&str] = &["line", "blankLine", "separator"];
pub const DEFAULT_CAPTURE_DELIMITER: &str = "blankLine";

impl CaptureDelimiter {
    pub fn as_str(self) -> &'static str {
        match self {
            CaptureDelimiter::Line => "line",
            CaptureDelimiter::BlankLine => "blankLine",
            CaptureDelimiter::Separator => "separator",
        }
    }
}

/// Resolves a stored delimiter to the supported set.
///
/// A hand-edited or corrupted `config.toml` falls back to the default rather
/// than leaving captures with no separator at all, the same way an unknown
/// theme falls back to following the system.
pub fn delimiter_name(value: &str) -> &'static str {
    CAPTURE_DELIMITERS
        .iter()
        .find(|name| **name == value)
        .copied()
        .unwrap_or(DEFAULT_CAPTURE_DELIMITER)
}

pub fn delimiter_from_name(value: &str) -> CaptureDelimiter {
    match delimiter_name(value) {
        "line" => CaptureDelimiter::Line,
        "separator" => CaptureDelimiter::Separator,
        _ => CaptureDelimiter::BlankLine,
    }
}

/// One armed run of AutoPaste: which note, and which generation.
///
/// Carried by every asynchronous clipboard read and checked again when the read
/// comes back, because everything that matters may have changed in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureSession {
    pub note_id: Uuid,
    pub generation: u64,
}

/// Why a clipboard change is not a capture. Reasons, never content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreReason {
    /// Nothing is armed. In practice unreachable, because the handler is
    /// disconnected while off — kept so the policy is total rather than
    /// relying on the connection for correctness.
    NotArmed,
    /// Note-it itself put this on the clipboard. This is the loop guard.
    OwnClipboard,
    /// The clipboard holds no text: an image, a file list, something binary.
    NotText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeDecision {
    Ignore(IgnoreReason),
    /// Read the clipboard now, under this session.
    Read(CaptureSession),
    /// A read is already running. This change is remembered and read after it,
    /// which is what keeps captures in the order they were copied.
    Queue,
}

/// What arming did: who has it now, and who lost it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmOutcome {
    pub session: CaptureSession,
    /// The note that was the target and is not any more, if there was one.
    pub released: Option<Uuid>,
}

/// The armed state of AutoPaste, for the whole application.
///
/// Deliberately one of these, not one per note: see `ChangeDecision` and
/// ADR-031. It holds no clipboard text, no hash of any, and nothing that
/// outlives the process.
#[derive(Debug, Default)]
pub struct AutoPaste {
    armed: Option<CaptureSession>,
    next_generation: u64,
    /// A clipboard read is in flight. One at a time, so results arrive in the
    /// order the copies happened.
    reading: bool,
    /// A change arrived while a read was in flight; read again when it lands.
    queued: bool,
}

impl AutoPaste {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn target(&self) -> Option<Uuid> {
        self.armed.map(|session| session.note_id)
    }

    pub fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    pub fn is_target(&self, note_id: Uuid) -> bool {
        self.target() == Some(note_id)
    }

    /// Whether a read is in flight. An observation of an internal detail, so
    /// the tests can state "nothing was even started" rather than inferring it.
    #[cfg(test)]
    pub fn is_reading(&self) -> bool {
        self.reading
    }

    /// Makes `note_id` the one target, releasing whatever held it before.
    ///
    /// Arming mints a new generation, which is what makes every read still in
    /// flight stale. A change that arrived while the previous target was armed
    /// and had not been read yet is dropped rather than handed to the new one:
    /// it was copied for a note that is no longer listening.
    pub fn arm(&mut self, note_id: Uuid) -> ArmOutcome {
        let released = self.target().filter(|previous| *previous != note_id);
        self.next_generation += 1;
        let session = CaptureSession {
            note_id,
            generation: self.next_generation,
        };
        self.armed = Some(session);
        self.queued = false;
        ArmOutcome { session, released }
    }

    /// Turns AutoPaste off, and returns the note that was the target.
    ///
    /// The generation moves on here too, so a read already in flight cannot
    /// deliver into a note nobody is capturing for any more — which is what
    /// makes disable, close, hide and quit safe against a callback in mid-air.
    pub fn disarm(&mut self) -> Option<Uuid> {
        let released = self.target();
        if released.is_some() {
            self.next_generation += 1;
        }
        self.armed = None;
        self.queued = false;
        released
    }

    /// Turns AutoPaste off only if `note_id` is the one holding it.
    ///
    /// Closing, trashing or hiding a note that never was the target must not
    /// silently switch capture off for the note that is.
    pub fn disarm_note(&mut self, note_id: Uuid) -> Option<Uuid> {
        if self.is_target(note_id) {
            self.disarm()
        } else {
            None
        }
    }

    /// The decision for one clipboard change.
    ///
    /// `own_clipboard` is GDK's own answer to "did this application put that
    /// there", and it is the loop protection: a `Ctrl+C` inside a note is Note-it
    /// claiming the clipboard, so the capture that would feed the note its own
    /// words back never starts. `has_text` gates on the offered formats, so an
    /// image is refused without reading a byte of it.
    pub fn observe(&mut self, own_clipboard: bool, has_text: bool) -> ChangeDecision {
        let Some(session) = self.armed else {
            return ChangeDecision::Ignore(IgnoreReason::NotArmed);
        };
        if own_clipboard {
            return ChangeDecision::Ignore(IgnoreReason::OwnClipboard);
        }
        if !has_text {
            return ChangeDecision::Ignore(IgnoreReason::NotText);
        }
        if self.reading {
            self.queued = true;
            return ChangeDecision::Queue;
        }
        self.reading = true;
        ChangeDecision::Read(session)
    }

    /// A read has come back. Returns the session to read under next, when a
    /// change arrived while this one was running.
    pub fn finish_read(&mut self) -> Option<CaptureSession> {
        self.reading = false;
        if !self.queued {
            return None;
        }
        self.queued = false;
        let session = self.armed?;
        self.reading = true;
        Some(session)
    }

    /// Revalidates a finished read against the state as it is *now*.
    ///
    /// `Some(note)` only when AutoPaste is still on, the target is still that
    /// note, and the generation has not moved. Everything else — disabled while
    /// reading, target switched, note closed, application hiding — leaves the
    /// capture undelivered, which is the point.
    pub fn accept(&self, session: CaptureSession) -> Option<Uuid> {
        match self.armed {
            Some(current) if current == session => Some(current.note_id),
            _ => None,
        }
    }
}

/// Whether captured text is worth inserting at all.
///
/// Empty is nothing. Whitespace alone is nothing anybody meant to file: it
/// would add a delimiter, a phantom line and a modification date for a copy
/// that carried no words. Everything else is the reader's text and is left
/// exactly as it is — no trimming of the content itself happens here or
/// anywhere else on the way in.
pub fn is_capturable(text: &str) -> bool {
    !text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note() -> Uuid {
        Uuid::new_v4()
    }

    #[test]
    fn a_fresh_application_is_not_capturing_anything() {
        let auto = AutoPaste::new();
        assert!(!auto.is_armed());
        assert_eq!(auto.target(), None);
        assert!(!auto.is_reading());
    }

    #[test]
    fn a_change_arriving_while_off_is_not_a_capture() {
        // Unreachable in the application, because the handler is disconnected
        // while off. Stated anyway: the policy must not depend on the
        // connection for its answer.
        let mut auto = AutoPaste::new();
        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Ignore(IgnoreReason::NotArmed)
        );
    }

    #[test]
    fn arming_makes_one_note_the_target() {
        let mut auto = AutoPaste::new();
        let a = note();
        let outcome = auto.arm(a);
        assert_eq!(outcome.released, None);
        assert_eq!(auto.target(), Some(a));
        assert!(auto.is_target(a));
        assert!(!auto.is_target(note()));
    }

    #[test]
    fn arming_a_second_note_releases_the_first() {
        // The system clipboard is one thing. Two notes capturing every copy
        // would double every `Ctrl+C`, which is both surprising and dangerous.
        let mut auto = AutoPaste::new();
        let (a, b) = (note(), note());
        auto.arm(a);
        let outcome = auto.arm(b);

        assert_eq!(outcome.released, Some(a));
        assert_eq!(auto.target(), Some(b));
        assert!(!auto.is_target(a));
    }

    #[test]
    fn arming_the_same_note_again_releases_nobody() {
        let mut auto = AutoPaste::new();
        let a = note();
        let first = auto.arm(a);
        let second = auto.arm(a);
        assert_eq!(second.released, None);
        // The generation still moves, so anything already in flight is stale.
        assert_ne!(first.session.generation, second.session.generation);
    }

    #[test]
    fn note_it_never_captures_its_own_clipboard() {
        // The loop guard. Selecting text in the target note and copying it
        // must not append that text to the same note.
        let mut auto = AutoPaste::new();
        auto.arm(note());
        assert_eq!(
            auto.observe(true, true),
            ChangeDecision::Ignore(IgnoreReason::OwnClipboard)
        );
        // ...and nothing was started, so nothing can arrive late either.
        assert!(!auto.is_reading());
    }

    #[test]
    fn a_clipboard_with_no_text_is_refused_without_being_read() {
        let mut auto = AutoPaste::new();
        auto.arm(note());
        assert_eq!(
            auto.observe(false, false),
            ChangeDecision::Ignore(IgnoreReason::NotText)
        );
        assert!(!auto.is_reading());
    }

    #[test]
    fn an_external_change_starts_one_read() {
        let mut auto = AutoPaste::new();
        let a = note();
        let armed = auto.arm(a);
        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Read(armed.session)
        );
        assert!(auto.is_reading());
    }

    #[test]
    fn a_change_during_a_read_waits_for_it_instead_of_racing_it() {
        // One read at a time is what keeps A, B, C arriving as A, B, C: two
        // reads in flight can finish in either order.
        let mut auto = AutoPaste::new();
        let a = note();
        let armed = auto.arm(a);

        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Read(armed.session)
        );
        assert_eq!(auto.observe(false, true), ChangeDecision::Queue);
        assert_eq!(auto.observe(false, true), ChangeDecision::Queue);

        // The queued changes collapse into one further read: the clipboard
        // holds one value, and the intermediate ones are already gone.
        let next = auto.finish_read().expect("the queued change is read next");
        assert_eq!(next, armed.session);
        assert!(auto.is_reading());
        assert_eq!(auto.finish_read(), None);
        assert!(!auto.is_reading());
    }

    #[test]
    fn a_read_that_lands_while_still_armed_is_delivered() {
        let mut auto = AutoPaste::new();
        let a = note();
        let armed = auto.arm(a);
        assert_eq!(auto.accept(armed.session), Some(a));
    }

    #[test]
    fn a_read_that_lands_after_autopaste_was_switched_off_is_dropped() {
        // Case: enabled, clipboard changes, read starts, reader switches
        // AutoPaste off, read finishes. Nothing may be inserted.
        let mut auto = AutoPaste::new();
        let a = note();
        let armed = auto.arm(a);
        auto.observe(false, true);
        assert_eq!(auto.disarm(), Some(a));
        assert_eq!(auto.accept(armed.session), None);
    }

    #[test]
    fn a_read_started_for_one_note_never_lands_in_another() {
        // Case: A armed, read starts, reader arms B, the old read finishes.
        // It must reach neither A nor B.
        let mut auto = AutoPaste::new();
        let (a, b) = (note(), note());
        let first = auto.arm(a);
        auto.observe(false, true);
        let second = auto.arm(b);

        assert_eq!(auto.accept(first.session), None);
        assert_eq!(auto.accept(second.session), Some(b));
        assert_ne!(first.session.generation, second.session.generation);
    }

    #[test]
    fn a_change_queued_for_the_old_target_is_not_handed_to_the_new_one() {
        let mut auto = AutoPaste::new();
        let (a, b) = (note(), note());
        auto.arm(a);
        auto.observe(false, true);
        auto.observe(false, true);

        auto.arm(b);
        // The queue was for A. B starts clean.
        assert_eq!(auto.finish_read(), None);
    }

    #[test]
    fn a_stale_read_landing_still_lets_the_next_one_start() {
        // The in-flight read for A comes back after B was armed. It delivers
        // nothing, but it must not leave the machine believing a read is still
        // running, or B would never capture anything again.
        let mut auto = AutoPaste::new();
        let (a, b) = (note(), note());
        let first = auto.arm(a);
        auto.observe(false, true);
        let second = auto.arm(b);

        assert_eq!(auto.accept(first.session), None);
        auto.finish_read();
        assert!(!auto.is_reading());
        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Read(second.session)
        );
    }

    #[test]
    fn disarming_a_note_that_never_held_it_leaves_the_target_alone() {
        let mut auto = AutoPaste::new();
        let (a, b) = (note(), note());
        auto.arm(a);
        assert_eq!(auto.disarm_note(b), None);
        assert_eq!(auto.target(), Some(a));
        assert_eq!(auto.disarm_note(a), Some(a));
        assert_eq!(auto.target(), None);
    }

    #[test]
    fn disarming_twice_is_not_two_releases() {
        let mut auto = AutoPaste::new();
        auto.arm(note());
        assert!(auto.disarm().is_some());
        assert_eq!(auto.disarm(), None);
    }

    #[test]
    fn two_identical_external_copies_are_two_captures() {
        // Deliberately no content comparison anywhere. Copying `ABC` twice, in
        // two separate actions, files it twice — which is what the reader
        // asked for both times.
        let mut auto = AutoPaste::new();
        let armed = auto.arm(note());

        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Read(armed.session)
        );
        assert_eq!(auto.accept(armed.session), Some(armed.session.note_id));
        auto.finish_read();

        assert_eq!(
            auto.observe(false, true),
            ChangeDecision::Read(armed.session)
        );
        assert_eq!(auto.accept(armed.session), Some(armed.session.note_id));
    }

    #[test]
    fn nothing_here_remembers_a_clipboard() {
        // The type is four small fields and none of them is text. There is no
        // last-clipboard, no hash and no buffer to leak, to persist or to have
        // to clear.
        let mut auto = AutoPaste::new();
        auto.arm(note());
        auto.observe(false, true);
        let rendered = format!("{auto:?}");
        assert!(!rendered.contains("text"));
        assert!(!rendered.contains("clipboard"));
    }

    #[test]
    fn only_text_with_something_in_it_is_worth_filing() {
        assert!(is_capturable("café"));
        assert!(is_capturable("  linha com espaços  "));
        assert!(is_capturable("日本語"));
        assert!(is_capturable("🧪"));

        assert!(!is_capturable(""));
        assert!(!is_capturable(" "));
        assert!(!is_capturable("\n"));
        assert!(!is_capturable("\t\r\n  "));
        assert!(!is_capturable("\u{00a0}"));
    }

    #[test]
    fn an_unknown_delimiter_degrades_to_the_default() {
        for unknown in ["", "linha", "BLANKLINE", "separator ", "regex"] {
            assert_eq!(delimiter_name(unknown), DEFAULT_CAPTURE_DELIMITER);
            assert_eq!(delimiter_from_name(unknown), CaptureDelimiter::BlankLine);
        }
        for name in CAPTURE_DELIMITERS {
            assert_eq!(delimiter_name(name), *name);
            assert_eq!(delimiter_from_name(name).as_str(), *name);
        }
    }

    #[test]
    fn the_default_delimiter_is_a_blank_line() {
        assert_eq!(CaptureDelimiter::default(), CaptureDelimiter::BlankLine);
        assert_eq!(
            CaptureDelimiter::default().as_str(),
            DEFAULT_CAPTURE_DELIMITER
        );
    }

    #[test]
    fn the_delimiter_spells_itself_the_way_the_page_does() {
        let encoded = serde_json::to_value(CaptureDelimiter::BlankLine).expect("serialize");
        assert_eq!(encoded, "blankLine");
        let decoded: CaptureDelimiter =
            serde_json::from_value(serde_json::json!("separator")).expect("deserialize");
        assert_eq!(decoded, CaptureDelimiter::Separator);
    }
}
