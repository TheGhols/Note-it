//! The note's Timer and Pomodoro, as the host sees them.
//!
//! The host does not run the countdown. It keeps the record and it rings the
//! bell: the state machine lives in `ui/src/timer/engine.ts`, where it can be
//! driven by a fake clock instead of by waiting. What is here is the part that
//! has to outlive a WebView — a small, validated value stored beside the
//! window geometry in `state.json`.
//!
//! Two properties are deliberate.
//!
//! **It is not content.** Nothing in this file touches a note's Markdown, its
//! `updated_at`, the search index, the title or the trash. A timer is
//! operational state of the application, in the same sense the note's position
//! and its zoom are, and it is stored in the same place for the same reason.
//!
//! **It is not trusted.** `state.json` is an ordinary file somebody can edit,
//! and the WebView is not trusted with the host's structures anywhere else in
//! Note-it either. Everything arriving from either direction goes through
//! [`NoteTimerState::sanitize`] before it is stored or sent, so a state that
//! claims to be running without an instant to run to comes back idle rather
//! than as a countdown against nothing.

use serde::{Deserialize, Serialize};

/// Whole minutes, matching `MIN_TIMER_MINUTES` in the page's engine.
pub const MIN_TIMER_MINUTES: u16 = 1;
/// Ten hours, matching `MAX_TIMER_MINUTES` in the page's engine.
pub const MAX_TIMER_MINUTES: u16 = 600;
pub const DEFAULT_TIMER_MINUTES: u16 = 25;
/// Four focus sessions make a cycle; the fourth is followed by the long break.
pub const FOCUS_SESSIONS_PER_CYCLE: u8 = 4;

const MAX_REMAINING_MS: i64 = MAX_TIMER_MINUTES as i64 * 60_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimerMode {
    #[default]
    Timer,
    Pomodoro,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TimerRunState {
    #[default]
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PomodoroPhase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
}

/// What finished, as a value from a closed set.
///
/// The page reports *which* run ended and never the words to show for it, so
/// there is no message on the wire that a note's text could travel in. The
/// sentences are [`TimerFinishKind::notification`], here, and they are the
/// only ones that can ever be posted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TimerFinishKind {
    Timer,
    Focus,
    ShortBreak,
    LongBreak,
}

impl TimerFinishKind {
    /// The notification's title and body.
    ///
    /// Short, fixed, and about the timer alone: no note title, no snippet, no
    /// Markdown, nothing the reader did not already know they had started.
    pub fn notification(self) -> (&'static str, Option<&'static str>) {
        match self {
            TimerFinishKind::Timer => ("Timer concluído", None),
            TimerFinishKind::Focus => ("Pomodoro", Some("Sessão de foco concluída.")),
            TimerFinishKind::ShortBreak | TimerFinishKind::LongBreak => {
                ("Pomodoro", Some("Pausa concluída."))
            }
        }
    }

    /// A stable identifier for the notification, so a second one replaces the
    /// first in the shell rather than stacking beside it.
    pub fn notification_id(self) -> &'static str {
        "note-it-timer"
    }
}

/// One note's timer, as it is stored and as it travels.
///
/// A run that is going is defined by `deadline_ms` — the wall-clock instant it
/// ends — and never by a remainder that something would have to keep
/// decrementing. That is what lets a note reopen after the application was
/// closed for ten minutes and show the fifteen that are actually left.
///
/// A paused run is the mirror: no deadline at all, and the frozen remainder in
/// `remaining_ms`. Paused time cannot be spent, because there is no instant
/// recorded for anything to have moved towards.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoteTimerState {
    #[serde(default)]
    pub mode: TimerMode,
    #[serde(default)]
    pub state: TimerRunState,
    #[serde(default = "default_timer_minutes")]
    pub timer_minutes: u16,
    /// Epoch milliseconds. Present only while running.
    #[serde(default)]
    pub deadline_ms: Option<i64>,
    /// Milliseconds left. Present only while paused.
    #[serde(default)]
    pub remaining_ms: Option<i64>,
    #[serde(default)]
    pub phase: PomodoroPhase,
    #[serde(default)]
    pub focus_completed: u8,
}

fn default_timer_minutes() -> u16 {
    DEFAULT_TIMER_MINUTES
}

impl Default for NoteTimerState {
    fn default() -> Self {
        Self {
            mode: TimerMode::default(),
            state: TimerRunState::default(),
            timer_minutes: DEFAULT_TIMER_MINUTES,
            deadline_ms: None,
            remaining_ms: None,
            phase: PomodoroPhase::default(),
            focus_completed: 0,
        }
    }
}

impl NoteTimerState {
    /// A timer state made safe to store, and dropped entirely when it says
    /// nothing.
    ///
    /// `None` means "this note has no timer": the pristine value is not written
    /// into `state.json`, so opening the panel and closing it again leaves the
    /// file exactly as it was.
    ///
    /// The rules are the ones the type only implies. Minutes and the session
    /// count are clamped into range. A state carries the field that defines it
    /// or it is not that state — a `Running` with no deadline and a `Paused`
    /// with no remainder are both damaged records, and they come back `Idle`
    /// rather than as a run against nothing. Fields belonging to the state a
    /// value is *not* in are cleared, so there is never a stale instant left
    /// on a paused timer for something to later count against.
    pub fn sanitize(self) -> Option<Self> {
        let mut clean = Self {
            mode: self.mode,
            state: self.state,
            timer_minutes: self
                .timer_minutes
                .clamp(MIN_TIMER_MINUTES, MAX_TIMER_MINUTES),
            deadline_ms: self.deadline_ms,
            remaining_ms: self
                .remaining_ms
                .map(|remaining| remaining.clamp(0, MAX_REMAINING_MS)),
            phase: self.phase,
            focus_completed: self.focus_completed.min(FOCUS_SESSIONS_PER_CYCLE),
        };

        match clean.state {
            TimerRunState::Running if clean.deadline_ms.is_none() => {
                clean.state = TimerRunState::Idle;
            }
            TimerRunState::Paused if clean.remaining_ms.is_none() => {
                clean.state = TimerRunState::Idle;
            }
            _ => {}
        }

        if clean.state != TimerRunState::Running {
            clean.deadline_ms = None;
        }
        if clean.state != TimerRunState::Paused {
            clean.remaining_ms = None;
        }

        if clean == Self::default() {
            None
        } else {
            Some(clean)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_with_no_timer_stores_nothing_at_all() {
        // Opening the panel, looking at it and closing it again must not put a
        // record in `state.json` — and must not rewrite the file to say so.
        assert_eq!(NoteTimerState::default().sanitize(), None);
    }

    #[test]
    fn a_running_state_without_a_deadline_is_not_a_running_timer() {
        // The shape a hand-edited state file, or a page that lost its way,
        // would produce. Restoring it as `Running` would leave a countdown with
        // nothing to count towards.
        let damaged = NoteTimerState {
            state: TimerRunState::Running,
            deadline_ms: None,
            ..NoteTimerState::default()
        };
        assert_eq!(damaged.sanitize(), None);
    }

    #[test]
    fn a_paused_state_without_a_remainder_is_not_a_paused_timer() {
        let damaged = NoteTimerState {
            state: TimerRunState::Paused,
            remaining_ms: None,
            ..NoteTimerState::default()
        };
        assert_eq!(damaged.sanitize(), None);
    }

    #[test]
    fn a_paused_timer_never_keeps_a_deadline_to_be_spent_later() {
        let stale = NoteTimerState {
            state: TimerRunState::Paused,
            remaining_ms: Some(90_000),
            // Left over from before the pause: the exact value that would make
            // paused time get spent anyway when the note is reopened.
            deadline_ms: Some(1_800_000_000_000),
            ..NoteTimerState::default()
        };
        let clean = stale.sanitize().expect("a paused run is worth storing");
        assert_eq!(clean.state, TimerRunState::Paused);
        assert_eq!(clean.remaining_ms, Some(90_000));
        assert_eq!(clean.deadline_ms, None);
    }

    #[test]
    fn a_running_timer_never_keeps_a_frozen_remainder() {
        let stale = NoteTimerState {
            state: TimerRunState::Running,
            deadline_ms: Some(1_800_000_000_000),
            remaining_ms: Some(90_000),
            ..NoteTimerState::default()
        };
        let clean = stale.sanitize().expect("a running run is worth storing");
        assert_eq!(clean.deadline_ms, Some(1_800_000_000_000));
        assert_eq!(clean.remaining_ms, None);
    }

    #[test]
    fn durations_and_session_counts_are_clamped_into_range() {
        let absurd = NoteTimerState {
            state: TimerRunState::Finished,
            timer_minutes: 0,
            focus_completed: 200,
            ..NoteTimerState::default()
        };
        let clean = absurd.sanitize().expect("a finished run is worth storing");
        assert_eq!(clean.timer_minutes, MIN_TIMER_MINUTES);
        assert_eq!(clean.focus_completed, FOCUS_SESSIONS_PER_CYCLE);

        let huge = NoteTimerState {
            state: TimerRunState::Finished,
            timer_minutes: u16::MAX,
            remaining_ms: Some(i64::MAX),
            ..NoteTimerState::default()
        };
        let clean = huge.sanitize().expect("a finished run is worth storing");
        assert_eq!(clean.timer_minutes, MAX_TIMER_MINUTES);
        // Finished is neither running nor paused, so neither instant survives.
        assert_eq!(clean.remaining_ms, None);
        assert_eq!(clean.deadline_ms, None);
    }

    #[test]
    fn a_paused_remainder_is_clamped_rather_than_believed() {
        let huge = NoteTimerState {
            state: TimerRunState::Paused,
            remaining_ms: Some(i64::MAX),
            ..NoteTimerState::default()
        };
        assert_eq!(
            huge.sanitize().expect("paused").remaining_ms,
            Some(MAX_REMAINING_MS)
        );

        let negative = NoteTimerState {
            state: TimerRunState::Paused,
            remaining_ms: Some(-5_000),
            ..NoteTimerState::default()
        };
        assert_eq!(negative.sanitize().expect("paused").remaining_ms, Some(0));
    }

    #[test]
    fn the_wire_spells_the_state_the_way_the_page_does() {
        // One vocabulary across the bridge. A rename on either side that the
        // other does not follow is a timer that silently comes back idle.
        let state = NoteTimerState {
            mode: TimerMode::Pomodoro,
            state: TimerRunState::Running,
            timer_minutes: 45,
            deadline_ms: Some(1_800_000_000_000),
            remaining_ms: None,
            phase: PomodoroPhase::ShortBreak,
            focus_completed: 2,
        };
        let encoded = serde_json::to_value(state).expect("serialize the timer state");
        assert_eq!(encoded["mode"], "pomodoro");
        assert_eq!(encoded["state"], "running");
        assert_eq!(encoded["timerMinutes"], 45);
        assert_eq!(encoded["deadlineMs"], 1_800_000_000_000_i64);
        assert_eq!(encoded["phase"], "shortBreak");
        assert_eq!(encoded["focusCompleted"], 2);
        assert!(encoded["remainingMs"].is_null());

        let decoded: NoteTimerState =
            serde_json::from_value(encoded).expect("read the timer state back");
        assert_eq!(decoded, state);
    }

    #[test]
    fn an_older_state_file_reads_as_a_note_with_no_timer() {
        // Every field defaults, so a `state.json` written before this phase is
        // still a valid one and nothing has to migrate it.
        let decoded: NoteTimerState = serde_json::from_str("{}").expect("an empty record");
        assert_eq!(decoded, NoteTimerState::default());
        assert_eq!(decoded.sanitize(), None);
    }

    #[test]
    fn a_completed_run_says_what_ended_and_nothing_about_the_note() {
        assert_eq!(
            TimerFinishKind::Timer.notification(),
            ("Timer concluído", None)
        );
        assert_eq!(
            TimerFinishKind::Focus.notification(),
            ("Pomodoro", Some("Sessão de foco concluída."))
        );
        assert_eq!(
            TimerFinishKind::ShortBreak.notification(),
            ("Pomodoro", Some("Pausa concluída."))
        );
        assert_eq!(
            TimerFinishKind::LongBreak.notification(),
            ("Pomodoro", Some("Pausa concluída."))
        );

        // The whole vocabulary, so nothing a note contains can reach a
        // notification: there is no variant carrying text at all.
        for kind in [
            TimerFinishKind::Timer,
            TimerFinishKind::Focus,
            TimerFinishKind::ShortBreak,
            TimerFinishKind::LongBreak,
        ] {
            let (title, body) = kind.notification();
            assert!(!title.is_empty());
            assert!(body.is_none_or(|text| !text.is_empty()));
            assert_eq!(kind.notification_id(), "note-it-timer");
        }
    }
}
