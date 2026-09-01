//! Durable study metadata, and no note content.
//!
//! A note still defines every flashcard. This file knows only an opaque
//! SHA-256 review key and the schedule attached to it. Questions, answers,
//! Markdown, HTML, titles, images and paths never enter this model.

use crate::atomic_file::write_atomic;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub const STUDY_VERSION: u32 = 1;
pub const STUDY_ALGORITHM: &str = "ladder-v1";
pub const MAX_LEVEL: u8 = 8;

/// Exact integer intervals, in seconds. No ease factor and no floating point.
pub const INTERVAL_SECONDS: [i64; 9] = [
    10 * 60,
    24 * 60 * 60,
    3 * 24 * 60 * 60,
    7 * 24 * 60 * 60,
    14 * 24 * 60 * 60,
    30 * 24 * 60 * 60,
    60 * 24 * 60 * 60,
    120 * 24 * 60 * 60,
    240 * 24 * 60 * 60,
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Difficult,
    Medium,
    Easy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StudyCardState {
    pub level: u8,
    pub due_at: DateTime<Utc>,
    pub last_reviewed_at: DateTime<Utc>,
    pub review_count: u64,
    pub last_rating: Rating,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StudyDay {
    pub reviews: u64,
    pub difficult: u64,
    pub medium: u64,
    pub easy: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StudyState {
    pub version: u32,
    pub algorithm: String,
    pub cards: BTreeMap<String, StudyCardState>,
    pub days: BTreeMap<NaiveDate, StudyDay>,
}

impl Default for StudyState {
    fn default() -> Self {
        Self {
            version: STUDY_VERSION,
            algorithm: STUDY_ALGORITHM.to_string(),
            cards: BTreeMap::new(),
            days: BTreeMap::new(),
        }
    }
}

fn valid_review_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl StudyState {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != STUDY_VERSION {
            return Err(format!(
                "study data version {} is not supported (expected {STUDY_VERSION})",
                self.version
            ));
        }
        if self.algorithm != STUDY_ALGORITHM {
            return Err(format!(
                "study algorithm {} is not supported",
                self.algorithm
            ));
        }
        for (key, card) in &self.cards {
            if !valid_review_key(key) {
                return Err("study data contains an invalid review key".to_string());
            }
            if card.level > MAX_LEVEL {
                return Err(format!("study data contains invalid level {}", card.level));
            }
        }
        Ok(())
    }
}

/// Missing is the empty history. Existing but unreadable is an error and is
/// never silently replaced.
pub fn load(path: &Path) -> Result<StudyState, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(StudyState::default()),
        Err(error) => return Err(format!("Failed to inspect study.json: {error}")),
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err("study.json is not a regular file".to_string())
        }
        Ok(_) => {}
    }

    let raw =
        fs::read_to_string(path).map_err(|error| format!("Failed to read study.json: {error}"))?;
    let state: StudyState =
        serde_json::from_str(&raw).map_err(|error| format!("study.json is invalid: {error}"))?;
    state.validate()?;
    Ok(state)
}

fn level_after(current: Option<u8>, rating: Rating) -> u8 {
    match (current, rating) {
        (None, Rating::Difficult) => 0,
        (None, Rating::Medium) => 1,
        (None, Rating::Easy) => 2,
        (Some(level), Rating::Difficult) => level.saturating_sub(1),
        (Some(level), Rating::Medium) => level.saturating_add(1).min(MAX_LEVEL),
        (Some(level), Rating::Easy) => level.saturating_add(2).min(MAX_LEVEL),
    }
}

pub fn scheduled_card(
    current: Option<&StudyCardState>,
    rating: Rating,
    now: DateTime<Utc>,
) -> StudyCardState {
    let level = level_after(current.map(|card| card.level), rating);
    StudyCardState {
        level,
        due_at: now + Duration::seconds(INTERVAL_SECONDS[usize::from(level)]),
        last_reviewed_at: now,
        review_count: current.map_or(1, |card| card.review_count.saturating_add(1)),
        last_rating: rating,
    }
}

fn apply_rating(
    state: &StudyState,
    review_key: &str,
    rating: Rating,
    now: DateTime<Utc>,
    local_day: NaiveDate,
) -> Result<StudyState, String> {
    state.validate()?;
    if !valid_review_key(review_key) {
        return Err("invalid review key".to_string());
    }

    let mut next = state.clone();
    let card = scheduled_card(next.cards.get(review_key), rating, now);
    next.cards.insert(review_key.to_string(), card);

    let day = next.days.entry(local_day).or_default();
    day.reviews = day.reviews.saturating_add(1);
    match rating {
        Rating::Difficult => day.difficult = day.difficult.saturating_add(1),
        Rating::Medium => day.medium = day.medium.saturating_add(1),
        Rating::Easy => day.easy = day.easy.saturating_add(1),
    }
    Ok(next)
}

fn persist_next(
    current: &StudyState,
    review_key: &str,
    rating: Rating,
    now: DateTime<Utc>,
    local_day: NaiveDate,
    write: impl FnOnce(&[u8]) -> Result<(), String>,
) -> Result<StudyState, String> {
    let next = apply_rating(current, review_key, rating, now, local_day)?;
    let mut serialized = serde_json::to_vec_pretty(&next)
        .map_err(|error| format!("Failed to serialize study.json: {error}"))?;
    serialized.push(b'\n');
    write(&serialized)?;
    Ok(next)
}

/// The returned state is the state that was committed. A failed write returns
/// no optimistic state for a caller to adopt.
pub fn rate_at(
    path: &Path,
    review_key: &str,
    rating: Rating,
    now: DateTime<Utc>,
    local_day: NaiveDate,
) -> Result<StudyState, String> {
    let current = load(path)?;
    persist_next(&current, review_key, rating, now, local_day, |bytes| {
        write_atomic(path, bytes, "study.json")
    })
}

pub fn rate_now(path: &Path, review_key: &str, rating: Rating) -> Result<StudyState, String> {
    let now = Local::now();
    rate_at(
        path,
        review_key,
        rating,
        now.with_timezone(&Utc),
        now.date_naive(),
    )
}

#[cfg(test)]
pub fn current_streak(days: &BTreeMap<NaiveDate, StudyDay>, today: NaiveDate) -> u32 {
    let start = if days.get(&today).is_some_and(|day| day.reviews > 0) {
        today
    } else {
        let yesterday = today - Duration::days(1);
        if days.get(&yesterday).is_some_and(|day| day.reviews > 0) {
            yesterday
        } else {
            return 0;
        }
    };

    let mut count = 0;
    let mut cursor = start;
    while days.get(&cursor).is_some_and(|day| day.reviews > 0) {
        count += 1;
        cursor -= Duration::days(1);
    }
    count
}

#[cfg(test)]
pub fn longest_streak(days: &BTreeMap<NaiveDate, StudyDay>) -> u32 {
    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<NaiveDate> = None;
    for (day, _activity) in days.iter().filter(|(_, activity)| activity.reviews > 0) {
        run = if previous.is_some_and(|prior| *day == prior + Duration::days(1)) {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        previous = Some(*day);
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn missing_file_is_an_empty_versioned_state() {
        let tmp = tempdir().unwrap();
        assert_eq!(
            load(&tmp.path().join("study.json")).unwrap(),
            StudyState::default()
        );
    }

    #[test]
    fn new_ratings_begin_at_the_three_promised_intervals() {
        let now = at("2026-08-30T12:00:00Z");
        let difficult = scheduled_card(None, Rating::Difficult, now);
        let medium = scheduled_card(None, Rating::Medium, now);
        let easy = scheduled_card(None, Rating::Easy, now);
        assert_eq!(
            (difficult.level, difficult.due_at),
            (0, now + Duration::minutes(10))
        );
        assert_eq!((medium.level, medium.due_at), (1, now + Duration::days(1)));
        assert_eq!((easy.level, easy.due_at), (2, now + Duration::days(3)));
    }

    #[test]
    fn studied_ratings_step_down_one_up_one_or_up_two() {
        let now = at("2026-08-30T12:00:00Z");
        let current = StudyCardState {
            level: 1,
            due_at: now,
            last_reviewed_at: now,
            review_count: 7,
            last_rating: Rating::Medium,
        };
        assert_eq!(
            scheduled_card(Some(&current), Rating::Difficult, now).level,
            0
        );
        assert_eq!(scheduled_card(Some(&current), Rating::Medium, now).level, 2);
        assert_eq!(scheduled_card(Some(&current), Rating::Easy, now).level, 3);
    }

    #[test]
    fn ladder_has_a_floor_and_a_ceiling() {
        let now = at("2026-08-30T12:00:00Z");
        let at_level = |level| StudyCardState {
            level,
            due_at: now,
            last_reviewed_at: now,
            review_count: 1,
            last_rating: Rating::Medium,
        };
        assert_eq!(
            scheduled_card(Some(&at_level(0)), Rating::Difficult, now).level,
            0
        );
        assert_eq!(
            scheduled_card(Some(&at_level(8)), Rating::Medium, now).level,
            8
        );
        assert_eq!(
            scheduled_card(Some(&at_level(8)), Rating::Easy, now).level,
            8
        );
    }

    #[test]
    fn due_time_count_rating_and_review_instant_are_exact() {
        let now = at("2026-08-30T12:34:56Z");
        let current = StudyCardState {
            level: 2,
            due_at: now,
            last_reviewed_at: now - Duration::days(1),
            review_count: 4,
            last_rating: Rating::Difficult,
        };
        let next = scheduled_card(Some(&current), Rating::Medium, now);
        assert_eq!(next.level, 3);
        assert_eq!(next.due_at, now + Duration::days(7));
        assert_eq!(next.review_count, 5);
        assert_eq!(next.last_rating, Rating::Medium);
        assert_eq!(next.last_reviewed_at, now);
    }

    #[test]
    fn rating_is_closed_and_invalid_levels_are_refused() {
        assert!(serde_json::from_str::<Rating>("\"almost\"").is_err());
        let mut state = StudyState::default();
        state.cards.insert(
            KEY.to_string(),
            StudyCardState {
                level: 9,
                due_at: Utc::now(),
                last_reviewed_at: Utc::now(),
                review_count: 1,
                last_rating: Rating::Easy,
            },
        );
        assert!(state.validate().is_err());
    }

    #[test]
    fn successful_rating_is_atomic_and_reloads() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("study.json");
        let now = at("2026-08-30T23:00:00Z");
        let written = rate_at(&path, KEY, Rating::Easy, now, day("2026-08-30")).unwrap();
        assert_eq!(load(&path).unwrap(), written);
        assert_eq!(written.days[&day("2026-08-30")].easy, 1);
        assert!(!fs::read_to_string(path).unwrap().contains("question"));
    }

    #[test]
    fn corrupt_or_future_data_is_not_replaced() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("study.json");
        fs::write(&path, "not json").unwrap();
        let before = fs::read(&path).unwrap();
        assert!(rate_at(&path, KEY, Rating::Medium, Utc::now(), day("2026-08-30")).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);

        fs::write(
            &path,
            r#"{"version":2,"algorithm":"ladder-v1","cards":{},"days":{}}"#,
        )
        .unwrap();
        let before = fs::read(&path).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);

        fs::write(
            &path,
            r#"{"version":1,"algorithm":"ladder-v1","cards":{},"days":{},"question":"must not be here"}"#,
        )
        .unwrap();
        let before = fs::read(&path).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn failed_persistence_does_not_return_an_optimistic_state() {
        let current = StudyState::default();
        let result = persist_next(
            &current,
            KEY,
            Rating::Medium,
            at("2026-08-30T12:00:00Z"),
            day("2026-08-30"),
            |_bytes| Err("disk full".to_string()),
        );
        assert!(result.is_err());
        assert!(current.cards.is_empty());
        assert!(current.days.is_empty());
    }

    #[test]
    fn daily_activity_uses_the_supplied_local_civil_day() {
        let state = StudyState::default();
        let first = apply_rating(
            &state,
            KEY,
            Rating::Difficult,
            Utc::now(),
            day("2026-08-30"),
        )
        .unwrap();
        let second =
            apply_rating(&first, KEY, Rating::Medium, Utc::now(), day("2026-08-30")).unwrap();
        let third =
            apply_rating(&second, KEY, Rating::Easy, Utc::now(), day("2026-08-31")).unwrap();
        assert_eq!(first.days[&day("2026-08-30")].reviews, 1);
        assert_eq!(
            second.days[&day("2026-08-30")],
            StudyDay {
                reviews: 2,
                difficult: 1,
                medium: 1,
                easy: 0
            }
        );
        assert_eq!(third.days[&day("2026-08-31")].easy, 1);
    }

    #[test]
    fn streaks_follow_civil_days_not_review_counts() {
        let mut days = BTreeMap::new();
        assert_eq!(current_streak(&days, day("2026-08-30")), 0);
        days.insert(
            day("2026-08-29"),
            StudyDay {
                reviews: 4,
                ..StudyDay::default()
            },
        );
        assert_eq!(current_streak(&days, day("2026-08-30")), 1);
        days.insert(
            day("2026-08-30"),
            StudyDay {
                reviews: 2,
                ..StudyDay::default()
            },
        );
        assert_eq!(current_streak(&days, day("2026-08-30")), 2);
        days.insert(
            day("2026-08-25"),
            StudyDay {
                reviews: 1,
                ..StudyDay::default()
            },
        );
        days.insert(
            day("2026-08-26"),
            StudyDay {
                reviews: 1,
                ..StudyDay::default()
            },
        );
        days.insert(
            day("2026-08-27"),
            StudyDay {
                reviews: 1,
                ..StudyDay::default()
            },
        );
        assert_eq!(longest_streak(&days), 3);
        assert_eq!(current_streak(&days, day("2026-09-02")), 0);
    }
}
