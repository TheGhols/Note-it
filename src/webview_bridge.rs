use crate::autopaste::CaptureDelimiter;
use crate::search::SearchResult;
use crate::study::{Rating, StudyState};
use crate::timer::{NoteTimerState, TimerFinishKind};
use crate::trash::TrashEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::WebView;

const MAX_GEOMETRY_DELTA: f64 = 100_000.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StudyCatalogNote {
    pub id: Uuid,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum HostToWebviewMessage {
    LoadNote {
        id: Uuid,
        content: String,
        color: String,
        #[serde(rename = "paperType")]
        paper_type: String,
        #[serde(rename = "paperIntensity")]
        paper_intensity: String,
        #[serde(rename = "fontSize")]
        font_size: u32,
        collapsed: bool,
        #[serde(rename = "createdAt")]
        created_at: Option<DateTime<Utc>>,
        #[serde(rename = "updatedAt")]
        updated_at: Option<DateTime<Utc>>,
        #[serde(rename = "zoomPercent")]
        zoom_percent: u16,
        #[serde(rename = "layerMode")]
        layer_mode: String,
        /// Shared interface theme, so a note dresses its chrome correctly from
        /// the first paint instead of restyling once the first broadcast lands.
        theme: String,
        /// The note's Timer or Pomodoro, or `null` when it has none.
        ///
        /// A running one travels as the instant it ends rather than as what
        /// was left when the note closed, so the page works out the remainder
        /// against the clock as it is now: a note reopened ten minutes later
        /// shows the ten minutes that really went by, and one reopened past
        /// its deadline comes back finished rather than counting through zero.
        timer: Option<NoteTimerState>,
        /// What AutoPaste would put between the note's content and a capture.
        ///
        /// A preference, so it travels with the note that is opening. Whether
        /// AutoPaste is *on* does not travel here and is never restored: a
        /// WebView that has just been created is never the capture target,
        /// because nothing survives the destruction of the previous one.
        #[serde(rename = "captureDelimiter")]
        capture_delimiter: CaptureDelimiter,
    },
    /// Sent when the host changes a note's collapse state, so the page and its
    /// menu follow a request that did not start in the WebView.
    SetCollapsed {
        collapsed: bool,
    },
    /// Broadcast whenever the shared layer mode changes, so every note's menu
    /// shows the same state.
    SetLayerMode {
        #[serde(rename = "layerMode")]
        layer_mode: String,
    },
    SetTimestamps {
        #[serde(rename = "createdAt")]
        created_at: Option<DateTime<Utc>>,
        #[serde(rename = "updatedAt")]
        updated_at: Option<DateTime<Utc>>,
    },
    SetColor {
        color: String,
    },
    /// Broadcast whenever the shared interface theme changes, so every note
    /// dresses its chrome the same way without being reloaded.
    SetTheme {
        theme: String,
    },
    /// Whether this note is the one AutoPaste is capturing into, and how a
    /// capture would be laid out.
    ///
    /// Pushed rather than asked for, because the host owns both: the target is
    /// exclusive across the application, so a note that loses it has to be told
    /// — its own menu and its own bar are still claiming it otherwise.
    SetAutoPaste {
        active: bool,
        delimiter: CaptureDelimiter,
    },
    /// One image is now in the store, and this is how the note refers to it.
    ///
    /// A path relative to `notes/` and nothing else — no absolute path, no
    /// bytes, no name from the machine it came from. The page puts it into the
    /// document and the ordinary autosave writes the note.
    ImageInserted {
        src: String,
    },
    /// An image could not be taken in. Already the sentence to show; the page
    /// never composes one, and it never names a file.
    ImageImportFailed {
        message: String,
    },
    /// One clipboard capture, on its way to the target note's editor.
    ///
    /// Text and nothing else: no HTML, no formats, no source, no timestamp.
    /// The page appends it at the end of the document as plain text and never
    /// takes the keyboard, moves the selection or scrolls to it.
    AutoPasteCaptured {
        text: String,
    },
    SetFontSize {
        #[serde(rename = "fontSize")]
        font_size: u32,
    },
    /// The answer to one `SearchRequested`. `request_id` is the page's own
    /// counter: a slow answer to an older query arrives carrying the number it
    /// was asked with, and the page drops it rather than letting it overwrite
    /// a newer one.
    SearchResults {
        #[serde(rename = "requestId")]
        request_id: u64,
        results: Vec<SearchResult>,
    },
    /// The note a search result named is no longer on disk — removed by
    /// another program between the search and the choice.
    SearchResultMissing {
        #[serde(rename = "noteId")]
        note_id: Uuid,
    },
    /// Sent to a note after search has brought it to the front, so the editor
    /// can find the occurrence itself. The query travels rather than a
    /// position: an offset into the stored Markdown is not an offset into the
    /// editor's document, and only the editor can turn one into the other.
    RevealMatch {
        query: String,
    },
    /// The answer to one `TrashListRequested`, numbered the same way a search
    /// answer is so a stale reply can be dropped rather than shown.
    TrashEntries {
        #[serde(rename = "requestId")]
        request_id: u64,
        entries: Vec<TrashEntry>,
    },
    /// What became of a data action the page asked for: moving a note to the
    /// trash, restoring one, or taking a backup. The message is already the
    /// sentence to show; the page never composes one from an error code.
    DataResult {
        action: String,
        ok: bool,
        message: String,
    },
    StudyCatalogResult {
        #[serde(rename = "requestId")]
        request_id: u64,
        notes: Vec<StudyCatalogNote>,
        #[serde(rename = "studyState")]
        study_state: Option<StudyState>,
        error: Option<String>,
    },
    StudyRatingResult {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "reviewKey")]
        review_key: String,
        ok: bool,
        #[serde(rename = "studyState")]
        study_state: Option<StudyState>,
        message: String,
    },
    RequestContent,
    RequestSaveAndClose,
    RequestFlush {
        #[serde(rename = "requestId")]
        request_id: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum WebviewToHostMessage {
    Ready,
    ContentChanged {
        id: Uuid,
        content: String,
    },
    SaveAndClose {
        id: Uuid,
        content: String,
    },
    NewNoteRequested,
    ColorChanged {
        id: Uuid,
        color: String,
    },
    FontSizeChanged {
        id: Uuid,
        #[serde(rename = "fontSize")]
        font_size: u32,
    },
    /// The note's own paper: its pattern and how strongly it is drawn. Both
    /// travel together because they describe one surface.
    PaperChanged {
        id: Uuid,
        #[serde(rename = "paperType")]
        paper_type: String,
        #[serde(rename = "paperIntensity")]
        paper_intensity: String,
    },
    /// Requests the shared interface theme. The host owns it, exactly as it
    /// owns the layer mode, so the WebView only asks.
    ThemeChanged {
        theme: String,
    },
    CollapseChanged {
        id: Uuid,
        collapsed: bool,
    },
    ZoomChanged {
        id: Uuid,
        #[serde(rename = "zoomPercent")]
        zoom_percent: u16,
    },
    /// Requests the shared Desktop/Overlay switch. The host owns the mode, so
    /// the WebView only asks for the toggle.
    ToggleLayerMode,
    /// Asks the host to search every stored note. Reading only: nothing about
    /// answering this writes a note, touches a timestamp or opens a window.
    SearchRequested {
        #[serde(rename = "requestId")]
        request_id: u64,
        query: String,
    },
    /// Asks the host to bring one search result to the front. The note is
    /// named by identifier, which `serde` will only accept as a UUID, so no
    /// path can be spelled here at all.
    OpenSearchResult {
        #[serde(rename = "noteId")]
        note_id: Uuid,
        query: String,
    },
    /// Asks the host to move **this** note to the trash. The page has already
    /// asked the reader to confirm; the host still flushes the note and only
    /// moves the file once that has succeeded.
    TrashNoteRequested {
        id: Uuid,
    },
    /// Asks for the contents of the trash. Reading only.
    TrashListRequested {
        #[serde(rename = "requestId")]
        request_id: u64,
    },
    /// Asks the host to bring one note back out of the trash. Named by
    /// identifier, which `serde` will only accept as a UUID, so no path can be
    /// spelled here at all.
    RestoreNoteRequested {
        #[serde(rename = "noteId")]
        note_id: Uuid,
    },
    /// Asks for a snapshot right now.
    BackupRequested,
    /// Requests the live/stored note documents and durable metadata. Rust does
    /// not parse cards; the existing ProseMirror extractor remains authority.
    StudyCatalogRequested {
        #[serde(rename = "requestId")]
        request_id: u64,
    },
    /// One opaque review key and one closed rating. The host owns the clock,
    /// applies Ladder-v1 and commits study.json before acknowledging.
    StudyRatingRequested {
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(rename = "reviewKey")]
        review_key: String,
        rating: Rating,
    },
    /// The note's timer changed in a way worth keeping: started, paused,
    /// resumed, cancelled, reset, moved to another phase, or finished.
    ///
    /// Never sent for a tick. The countdown is redrawn about once a second and
    /// none of that reaches the host, so a running timer costs no message and
    /// no write; what is stored is the deadline, which does not change while it
    /// is being counted down to. `null` means the note has no timer any more.
    TimerChanged {
        id: Uuid,
        timer: Option<NoteTimerState>,
    },
    /// Asks the host to show a file chooser and put the chosen image in this
    /// note. The host opens it, reads it and decides: no path is named here,
    /// in either direction.
    InsertImageRequested {
        id: Uuid,
    },
    /// Bytes of an image the reader pasted or dropped, base64 on the wire.
    ///
    /// The page sends what the gesture handed it rather than naming a file, so
    /// there is no path here for the host to be talked into reading. What the
    /// bytes actually are is decided by the host, from the bytes.
    ImageBytesReceived {
        id: Uuid,
        data: String,
    },
    /// Asks the host to make this note the AutoPaste target, or to stop
    /// capturing altogether.
    ///
    /// Only ever a request. The host owns the single target, so turning it on
    /// here is what turns it off wherever it was before, and the answer comes
    /// back as `SetAutoPaste` to both notes rather than being assumed here.
    AutoPasteRequested {
        id: Uuid,
        active: bool,
    },
    /// Asks the host to store a different capture delimiter. Application-wide,
    /// like the theme, so it is stored once and broadcast.
    CaptureDelimiterChanged {
        delimiter: CaptureDelimiter,
    },
    /// A run reached zero, exactly once, and the host should say so.
    ///
    /// The page reports *which* run ended and never the words for it, so the
    /// notification cannot be made to carry anything from the note.
    TimerFinished {
        id: Uuid,
        kind: TimerFinishKind,
    },
    OpenExternalUrl {
        url: String,
    },
    DragStart,
    DragUpdate {
        dx: f64,
        dy: f64,
    },
    DragEnd,
    ResizeStart,
    ResizeUpdate {
        dx: f64,
        dy: f64,
    },
    ResizeEnd,
    FlushResponse {
        id: Uuid,
        #[serde(rename = "requestId")]
        request_id: u64,
        content: String,
    },
}

pub fn send_to_webview(webview: &WebView, message: &HostToWebviewMessage) {
    if let Ok(json_str) = serde_json::to_string(message) {
        let script = format!("window.handleHostMessage && window.handleHostMessage({json_str});");
        webview.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }
}

pub fn parse_webview_message(raw_json: &str) -> Result<WebviewToHostMessage, String> {
    let message = serde_json::from_str::<WebviewToHostMessage>(raw_json)
        .map_err(|e| format!("Failed to parse webview message: {e}"))?;
    match &message {
        WebviewToHostMessage::DragUpdate { dx, dy }
        | WebviewToHostMessage::ResizeUpdate { dx, dy } => validate_geometry_delta(*dx, *dy)?,
        _ => {}
    }
    Ok(message)
}

fn validate_geometry_delta(dx: f64, dy: f64) -> Result<(), String> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err("Geometry delta must be finite".to_string());
    }
    if dx.abs() > MAX_GEOMETRY_DELTA || dy.abs() > MAX_GEOMETRY_DELTA {
        return Err("Geometry delta exceeds the allowed range".to_string());
    }
    Ok(())
}

pub fn validate_external_url(url: &str) -> Result<(), String> {
    if url.is_empty() || url.trim() != url || url.chars().any(char::is_control) {
        return Err("External URL contains invalid whitespace or control characters".to_string());
    }

    let parsed = glib::Uri::parse(url, glib::UriFlags::NONE)
        .map_err(|_| "External URL is malformed".to_string())?;
    let scheme = parsed.scheme().to_ascii_lowercase();

    match scheme.as_str() {
        "http" | "https" if parsed.host().is_some_and(|host| !host.is_empty()) => Ok(()),
        "mailto" if !parsed.path().is_empty() => Ok(()),
        "http" | "https" | "mailto" => Err("External URL is missing its destination".to_string()),
        _ => Err("External URL scheme is not allowed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_webview_message, validate_external_url, validate_geometry_delta, WebviewToHostMessage,
    };
    use uuid::Uuid;

    #[test]
    fn parses_save_and_close_as_one_message_with_latest_content() {
        let id = Uuid::new_v4();
        let raw = serde_json::json!({
            "type": "save_and_close",
            "payload": { "id": id, "content": "latest character: x" }
        })
        .to_string();

        let message = parse_webview_message(&raw).expect("save-and-close message");
        match message {
            WebviewToHostMessage::SaveAndClose {
                id: parsed_id,
                content,
            } => {
                assert_eq!(parsed_id, id);
                assert_eq!(content, "latest character: x");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn allows_explicit_external_url_schemes() {
        assert!(validate_external_url("https://example.com/path?q=1").is_ok());
        assert!(validate_external_url("http://example.com").is_ok());
        assert!(validate_external_url("mailto:person@example.com").is_ok());
    }

    #[test]
    fn blocks_unapproved_or_malformed_external_urls() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,test",
            "vbscript:msgbox(1)",
            "ftp://example.com",
            "ssh://example.com",
            "obsidian://open?vault=test",
            "custom://test",
            "custom-protocol://something",
            "https:",
            " https://example.com",
            "https://example.com\nfile:///etc/passwd",
        ] {
            assert!(
                validate_external_url(url).is_err(),
                "should block unsupported or malicious scheme {url:?}"
            );
        }
    }

    #[test]
    fn parses_fractional_drag_and_resize_messages() {
        let drag_json = serde_json::json!({
            "type": "drag_update",
            "payload": { "dx": 9.9140625, "dy": -0.87109375 }
        })
        .to_string();
        let drag_msg = parse_webview_message(&drag_json).expect("drag message");
        match drag_msg {
            WebviewToHostMessage::DragUpdate { dx, dy } => {
                assert_eq!(dx, 9.9140625);
                assert_eq!(dy, -0.87109375);
            }
            other => panic!("unexpected message: {other:?}"),
        }

        let resize_json = serde_json::json!({
            "type": "resize_update",
            "payload": { "dx": 50.25, "dy": -40.75 }
        })
        .to_string();
        let resize_msg = parse_webview_message(&resize_json).expect("resize message");
        match resize_msg {
            WebviewToHostMessage::ResizeUpdate { dx, dy } => {
                assert_eq!(dx, 50.25);
                assert_eq!(dy, -40.75);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_geometry_deltas() {
        assert!(validate_geometry_delta(f64::NAN, 0.0).is_err());
        assert!(validate_geometry_delta(0.0, f64::INFINITY).is_err());
        for payload in [
            serde_json::json!({ "dx": 100_000.1, "dy": 0 }),
            serde_json::json!({ "dx": 0, "dy": -100_000.1 }),
            serde_json::json!({ "dx": "NaN", "dy": 0 }),
        ] {
            let raw = serde_json::json!({
                "type": "drag_update",
                "payload": payload,
            })
            .to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }
    }

    #[test]
    fn parses_flush_response_message() {
        let id = Uuid::new_v4();
        let flush_json = serde_json::json!({
            "type": "flush_response",
            "payload": {
                "id": id,
                "requestId": 42,
                "content": "# flushed content immediately"
            }
        })
        .to_string();

        let flush_msg = parse_webview_message(&flush_json).expect("flush message");
        match flush_msg {
            WebviewToHostMessage::FlushResponse {
                id: parsed_id,
                request_id,
                content,
            } => {
                assert_eq!(parsed_id, id);
                assert_eq!(request_id, 42);
                assert_eq!(content, "# flushed content immediately");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn study_requests_carry_only_generation_key_and_closed_rating() {
        match parse_webview_message(
            r#"{"type":"study_catalog_requested","payload":{"requestId":17}}"#,
        )
        .expect("catalog request")
        {
            WebviewToHostMessage::StudyCatalogRequested { request_id } => {
                assert_eq!(request_id, 17)
            }
            other => panic!("unexpected message: {other:?}"),
        }

        let key = "a".repeat(64);
        let raw = serde_json::json!({
            "type": "study_rating_requested",
            "payload": { "requestId": 18, "reviewKey": key, "rating": "difficult" }
        })
        .to_string();
        match parse_webview_message(&raw).expect("rating request") {
            WebviewToHostMessage::StudyRatingRequested {
                request_id,
                review_key,
                rating,
            } => {
                assert_eq!(request_id, 18);
                assert_eq!(review_key, "a".repeat(64));
                assert_eq!(rating, crate::study::Rating::Difficult);
            }
            other => panic!("unexpected message: {other:?}"),
        }

        let invalid = raw.replace("difficult", "almost");
        assert!(parse_webview_message(&invalid).is_err());
    }

    #[test]
    fn study_results_keep_request_ids_and_use_the_frontend_field_names() {
        let catalog = super::HostToWebviewMessage::StudyCatalogResult {
            request_id: 41,
            notes: vec![super::StudyCatalogNote {
                id: Uuid::nil(),
                content: "A :: B".to_string(),
            }],
            study_state: Some(crate::study::StudyState::default()),
            error: None,
        };
        let encoded = serde_json::to_value(catalog).expect("catalog result");
        assert_eq!(encoded["type"], "study_catalog_result");
        assert_eq!(encoded["payload"]["requestId"], 41);
        assert_eq!(
            encoded["payload"]["notes"][0]["id"],
            Uuid::nil().to_string()
        );
        assert_eq!(encoded["payload"]["notes"][0]["content"], "A :: B");
        assert_eq!(encoded["payload"]["studyState"]["version"], 1);

        let rating = super::HostToWebviewMessage::StudyRatingResult {
            request_id: 42,
            review_key: "b".repeat(64),
            ok: false,
            study_state: None,
            message: "Não foi possível salvar.".to_string(),
        };
        let encoded = serde_json::to_value(rating).expect("rating result");
        assert_eq!(encoded["payload"]["requestId"], 42);
        assert_eq!(encoded["payload"]["reviewKey"], "b".repeat(64));
        assert_eq!(encoded["payload"]["ok"], false);
        assert!(encoded["payload"]["studyState"].is_null());
    }

    #[test]
    fn parses_collapse_change_requests_from_the_note_menu() {
        let id = Uuid::new_v4();
        for collapsed in [true, false] {
            let raw = serde_json::json!({
                "type": "collapse_changed",
                "payload": { "id": id, "collapsed": collapsed }
            })
            .to_string();

            match parse_webview_message(&raw).expect("collapse message") {
                WebviewToHostMessage::CollapseChanged {
                    id: parsed_id,
                    collapsed: parsed_collapsed,
                } => {
                    assert_eq!(parsed_id, id);
                    assert_eq!(parsed_collapsed, collapsed);
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }
    }

    #[test]
    fn load_note_carries_collapse_state_and_timestamps_to_the_webview() {
        let id = Uuid::new_v4();
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-08-27T07:14:00Z")
            .expect("fixed timestamp")
            .with_timezone(&chrono::Utc);

        let message = super::HostToWebviewMessage::LoadNote {
            id,
            content: "conteúdo".to_string(),
            color: "yellow".to_string(),
            paper_type: "grid-small".to_string(),
            paper_intensity: "subtle".to_string(),
            font_size: 15,
            collapsed: true,
            created_at: Some(created_at),
            updated_at: None,
            zoom_percent: 130,
            layer_mode: "desktop".to_string(),
            theme: "dark".to_string(),
            timer: None,
            capture_delimiter: crate::autopaste::CaptureDelimiter::BlankLine,
        };

        let encoded = serde_json::to_value(&message).expect("serialize load_note");
        let payload = &encoded["payload"];
        assert_eq!(encoded["type"], "load_note");
        assert_eq!(payload["collapsed"], true);
        assert_eq!(payload["createdAt"], "2026-08-27T07:14:00Z");
        assert_eq!(payload["zoomPercent"], 130);
        assert_eq!(payload["layerMode"], "desktop");
        assert_eq!(payload["paperType"], "grid-small");
        assert_eq!(payload["paperIntensity"], "subtle");
        assert_eq!(payload["theme"], "dark");
        // An unknown timestamp travels as null instead of a fabricated date.
        assert!(payload["updatedAt"].is_null());
    }

    #[test]
    fn a_note_with_no_timer_says_so_rather_than_inventing_one() {
        let message = super::HostToWebviewMessage::LoadNote {
            id: Uuid::new_v4(),
            content: String::new(),
            color: "yellow".to_string(),
            paper_type: "blank".to_string(),
            paper_intensity: "normal".to_string(),
            font_size: 15,
            collapsed: false,
            created_at: None,
            updated_at: None,
            zoom_percent: 100,
            layer_mode: "overlay".to_string(),
            theme: "system".to_string(),
            timer: None,
            capture_delimiter: crate::autopaste::CaptureDelimiter::BlankLine,
        };
        let encoded = serde_json::to_value(&message).expect("serialize load_note");
        assert!(encoded["payload"]["timer"].is_null());
    }

    #[test]
    fn a_running_timer_travels_as_the_instant_it_ends() {
        // The deadline is what makes a reopened note honest: it is an absolute
        // moment, so the page subtracts the clock as it is *now* rather than
        // resuming whatever was left when the WebView went away.
        let deadline = 1_800_000_600_000_i64;
        let message = super::HostToWebviewMessage::LoadNote {
            id: Uuid::new_v4(),
            content: "conteúdo".to_string(),
            color: "yellow".to_string(),
            paper_type: "blank".to_string(),
            paper_intensity: "normal".to_string(),
            font_size: 15,
            collapsed: false,
            created_at: None,
            updated_at: None,
            zoom_percent: 100,
            layer_mode: "overlay".to_string(),
            theme: "system".to_string(),
            timer: Some(crate::timer::NoteTimerState {
                state: crate::timer::TimerRunState::Running,
                deadline_ms: Some(deadline),
                ..crate::timer::NoteTimerState::default()
            }),
            capture_delimiter: crate::autopaste::CaptureDelimiter::BlankLine,
        };
        let encoded = serde_json::to_value(&message).expect("serialize load_note");
        let timer = &encoded["payload"]["timer"];
        assert_eq!(timer["state"], "running");
        assert_eq!(timer["deadlineMs"], deadline);
        // No remainder travels with a running run: there is one source of
        // truth on the wire, and it is the instant.
        assert!(timer["remainingMs"].is_null());
    }

    #[test]
    fn parses_the_timer_messages_a_note_can_send_about_itself() {
        let id = Uuid::new_v4();

        let raw = serde_json::json!({
            "type": "timer_changed",
            "payload": {
                "id": id,
                "timer": {
                    "mode": "pomodoro",
                    "state": "paused",
                    "timerMinutes": 25,
                    "deadlineMs": null,
                    "remainingMs": 742_000,
                    "phase": "focus",
                    "focusCompleted": 1
                }
            }
        })
        .to_string();
        match parse_webview_message(&raw).expect("timer change") {
            WebviewToHostMessage::TimerChanged {
                id: parsed_id,
                timer,
            } => {
                assert_eq!(parsed_id, id);
                let timer = timer.expect("a paused run travels");
                assert_eq!(timer.state, crate::timer::TimerRunState::Paused);
                assert_eq!(timer.remaining_ms, Some(742_000));
                assert_eq!(timer.focus_completed, 1);
            }
            other => panic!("unexpected message: {other:?}"),
        }

        // Clearing a note's timer is the same message carrying nothing.
        let raw = serde_json::json!({
            "type": "timer_changed",
            "payload": { "id": id, "timer": null }
        })
        .to_string();
        match parse_webview_message(&raw).expect("timer cleared") {
            WebviewToHostMessage::TimerChanged { timer, .. } => assert!(timer.is_none()),
            other => panic!("unexpected message: {other:?}"),
        }

        let raw = serde_json::json!({
            "type": "timer_finished",
            "payload": { "id": id, "kind": "focus" }
        })
        .to_string();
        match parse_webview_message(&raw).expect("timer finished") {
            WebviewToHostMessage::TimerFinished {
                id: parsed_id,
                kind,
            } => {
                assert_eq!(parsed_id, id);
                assert_eq!(kind, crate::timer::TimerFinishKind::Focus);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn a_completion_cannot_carry_words_of_its_own() {
        // The only thing a page may say about a finished run is which kind it
        // was. There is no field for a title, a body or a snippet, so a note's
        // text has no route to the desktop's notification area at all.
        for kind in ["timer", "focus", "shortBreak", "longBreak"] {
            let raw = serde_json::json!({
                "type": "timer_finished",
                "payload": { "id": Uuid::new_v4(), "kind": kind }
            })
            .to_string();
            assert!(parse_webview_message(&raw).is_ok(), "rejected {kind}");
        }

        for payload in [
            serde_json::json!({ "id": Uuid::new_v4(), "kind": "# a nota inteira" }),
            serde_json::json!({ "id": Uuid::new_v4(), "kind": { "title": "x", "body": "y" } }),
            serde_json::json!({ "id": Uuid::new_v4(), "kind": "timer", "body": "senha" }),
        ] {
            let raw =
                serde_json::json!({ "type": "timer_finished", "payload": payload }).to_string();
            let parsed = parse_webview_message(&raw);
            match parsed {
                Err(_) => {}
                Ok(WebviewToHostMessage::TimerFinished { kind, .. }) => {
                    // An extra field is ignored rather than carried: whatever
                    // was smuggled alongside is simply not part of the message.
                    assert_eq!(kind, crate::timer::TimerFinishKind::Timer);
                }
                Ok(other) => panic!("unexpected message: {other:?}"),
            }
        }
    }

    #[test]
    fn a_timer_state_from_the_page_is_never_trusted_as_it_arrives() {
        let id = Uuid::new_v4();
        let raw = serde_json::json!({
            "type": "timer_changed",
            "payload": {
                "id": id,
                "timer": {
                    "mode": "timer",
                    "state": "running",
                    "timerMinutes": 999,
                    "deadlineMs": null,
                    "focusCompleted": 99
                }
            }
        })
        .to_string();

        match parse_webview_message(&raw).expect("timer change") {
            WebviewToHostMessage::TimerChanged { timer, .. } => {
                let arrived = timer.expect("the payload parses");
                // As it arrives it is nonsense: running, with nothing to run
                // to, for a duration outside the supported range. Sanitising
                // is what the window does before any of it is stored.
                assert_eq!(arrived.state, crate::timer::TimerRunState::Running);
                let stored = arrived
                    .sanitize()
                    .expect("clamped values still say something");
                assert_eq!(stored.state, crate::timer::TimerRunState::Idle);
                assert_eq!(stored.timer_minutes, crate::timer::MAX_TIMER_MINUTES);
                assert_eq!(
                    stored.focus_completed,
                    crate::timer::FOCUS_SESSIONS_PER_CYCLE
                );
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn a_note_is_told_whether_it_is_the_capture_target() {
        // Pushed rather than asked for: the target is exclusive across the
        // application, so the note that just lost it has to hear so.
        for active in [true, false] {
            let encoded = serde_json::to_value(super::HostToWebviewMessage::SetAutoPaste {
                active,
                delimiter: crate::autopaste::CaptureDelimiter::Separator,
            })
            .expect("serialize set_autopaste");
            assert_eq!(encoded["type"], "set_auto_paste");
            assert_eq!(encoded["payload"]["active"], active);
            assert_eq!(encoded["payload"]["delimiter"], "separator");
        }
    }

    #[test]
    fn a_capture_travels_as_text_and_nothing_else() {
        // No formats, no source, no timestamp, no HTML. The page is handed the
        // words and appends them; there is no field here for anything that
        // would let a capture become markup or metadata.
        let encoded = serde_json::to_value(super::HostToWebviewMessage::AutoPasteCaptured {
            text: "encefalopatia hepática 🧪\nsegunda linha".to_string(),
        })
        .expect("serialize auto_paste_captured");
        assert_eq!(encoded["type"], "auto_paste_captured");
        assert_eq!(
            encoded["payload"]["text"],
            "encefalopatia hepática 🧪\nsegunda linha"
        );
        let payload = encoded["payload"].as_object().expect("an object");
        assert_eq!(payload.len(), 1, "a capture carries only its text");
    }

    #[test]
    fn a_note_can_only_ask_about_its_own_capture() {
        let id = Uuid::new_v4();
        for active in [true, false] {
            let raw = serde_json::json!({
                "type": "auto_paste_requested",
                "payload": { "id": id, "active": active }
            })
            .to_string();
            match parse_webview_message(&raw).expect("autopaste request") {
                WebviewToHostMessage::AutoPasteRequested {
                    id: parsed,
                    active: parsed_active,
                } => {
                    assert_eq!(parsed, id);
                    assert_eq!(parsed_active, active);
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }

        // A path is not a value that can be spelled here either.
        for payload in [
            serde_json::json!({ "id": "../../etc/passwd", "active": true }),
            serde_json::json!({ "id": 42, "active": true }),
            serde_json::json!({ "id": id, "active": "sim" }),
        ] {
            let raw = serde_json::json!({ "type": "auto_paste_requested", "payload": payload })
                .to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }
    }

    #[test]
    fn the_delimiter_is_a_choice_from_a_closed_set() {
        for name in crate::autopaste::CAPTURE_DELIMITERS {
            let raw = serde_json::json!({
                "type": "capture_delimiter_changed",
                "payload": { "delimiter": name }
            })
            .to_string();
            match parse_webview_message(&raw).expect("delimiter change") {
                WebviewToHostMessage::CaptureDelimiterChanged { delimiter } => {
                    assert_eq!(delimiter.as_str(), *name);
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }

        // Anything outside the set is refused by `serde` rather than reaching
        // the configuration: there is no template language here to smuggle.
        for unknown in [
            serde_json::json!("regex:.*"),
            serde_json::json!("\n\n---\n\n"),
            serde_json::json!(7),
            serde_json::json!({ "custom": "x" }),
        ] {
            let raw = serde_json::json!({
                "type": "capture_delimiter_changed",
                "payload": { "delimiter": unknown }
            })
            .to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }
    }

    #[test]
    fn a_note_opening_is_told_the_layout_but_never_that_capture_was_on() {
        // There is no field on `LoadNote` that could switch capture back on,
        // which is what makes "AutoPaste is off after a restart" a property of
        // the protocol rather than a promise about the code.
        let message = super::HostToWebviewMessage::LoadNote {
            id: Uuid::new_v4(),
            content: String::new(),
            color: "yellow".to_string(),
            paper_type: "blank".to_string(),
            paper_intensity: "normal".to_string(),
            font_size: 15,
            collapsed: false,
            created_at: None,
            updated_at: None,
            zoom_percent: 100,
            layer_mode: "overlay".to_string(),
            theme: "system".to_string(),
            timer: None,
            capture_delimiter: crate::autopaste::CaptureDelimiter::Line,
        };
        let encoded = serde_json::to_value(&message).expect("serialize load_note");
        let payload = encoded["payload"].as_object().expect("an object");
        assert_eq!(payload["captureDelimiter"], "line");
        for forbidden in ["autoPaste", "autoPasteActive", "capturing", "captureTarget"] {
            assert!(
                !payload.contains_key(forbidden),
                "load_note carries {forbidden}"
            );
        }
    }

    #[test]
    fn collapse_state_is_pushed_back_to_the_webview() {
        for collapsed in [true, false] {
            let encoded =
                serde_json::to_value(super::HostToWebviewMessage::SetCollapsed { collapsed })
                    .expect("serialize set_collapsed");
            assert_eq!(encoded["type"], "set_collapsed");
            assert_eq!(encoded["payload"]["collapsed"], collapsed);
        }
    }

    #[test]
    fn parses_zoom_change_requests() {
        let id = Uuid::new_v4();
        let raw = serde_json::json!({
            "type": "zoom_changed",
            "payload": { "id": id, "zoomPercent": 130 }
        })
        .to_string();

        match parse_webview_message(&raw).expect("zoom message") {
            WebviewToHostMessage::ZoomChanged {
                id: parsed_id,
                zoom_percent,
            } => {
                assert_eq!(parsed_id, id);
                assert_eq!(zoom_percent, 130);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn rejects_a_zoom_value_that_is_not_a_plain_percentage() {
        for payload in [
            serde_json::json!({ "id": Uuid::new_v4(), "zoomPercent": -10 }),
            serde_json::json!({ "id": Uuid::new_v4(), "zoomPercent": "NaN" }),
            serde_json::json!({ "id": Uuid::new_v4(), "zoomPercent": 99999999 }),
        ] {
            let raw = serde_json::json!({ "type": "zoom_changed", "payload": payload }).to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }
    }

    #[test]
    fn parses_paper_change_requests_from_the_note_menu() {
        let id = Uuid::new_v4();
        let raw = serde_json::json!({
            "type": "paper_changed",
            "payload": { "id": id, "paperType": "lined", "paperIntensity": "strong" }
        })
        .to_string();

        match parse_webview_message(&raw).expect("paper message") {
            WebviewToHostMessage::PaperChanged {
                id: parsed_id,
                paper_type,
                paper_intensity,
            } => {
                assert_eq!(parsed_id, id);
                assert_eq!(paper_type, "lined");
                assert_eq!(paper_intensity, "strong");
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn parses_theme_change_requests() {
        for theme in ["system", "light", "dark"] {
            let raw = serde_json::json!({ "type": "theme_changed", "payload": { "theme": theme } })
                .to_string();
            match parse_webview_message(&raw).expect("theme message") {
                WebviewToHostMessage::ThemeChanged { theme: parsed } => {
                    assert_eq!(parsed, theme);
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }
    }

    #[test]
    fn the_theme_is_pushed_back_to_every_webview() {
        for theme in ["system", "light", "dark"] {
            let encoded = serde_json::to_value(super::HostToWebviewMessage::SetTheme {
                theme: theme.to_string(),
            })
            .expect("serialize set_theme");
            assert_eq!(encoded["type"], "set_theme");
            assert_eq!(encoded["payload"]["theme"], theme);
        }
    }

    #[test]
    fn parses_layer_mode_toggle_requests() {
        let raw = serde_json::json!({ "type": "toggle_layer_mode" }).to_string();
        assert!(matches!(
            parse_webview_message(&raw).expect("layer toggle"),
            WebviewToHostMessage::ToggleLayerMode
        ));
    }

    #[test]
    fn parses_the_data_requests_the_menu_can_make() {
        let id = Uuid::new_v4();

        let raw = serde_json::json!({
            "type": "trash_note_requested",
            "payload": { "id": id }
        })
        .to_string();
        match parse_webview_message(&raw).expect("trash request") {
            WebviewToHostMessage::TrashNoteRequested { id: parsed } => assert_eq!(parsed, id),
            other => panic!("unexpected message: {other:?}"),
        }

        let raw = serde_json::json!({
            "type": "trash_list_requested",
            "payload": { "requestId": 7 }
        })
        .to_string();
        match parse_webview_message(&raw).expect("trash list request") {
            WebviewToHostMessage::TrashListRequested { request_id } => assert_eq!(request_id, 7),
            other => panic!("unexpected message: {other:?}"),
        }

        let raw = serde_json::json!({
            "type": "restore_note_requested",
            "payload": { "noteId": id }
        })
        .to_string();
        match parse_webview_message(&raw).expect("restore request") {
            WebviewToHostMessage::RestoreNoteRequested { note_id } => assert_eq!(note_id, id),
            other => panic!("unexpected message: {other:?}"),
        }

        let raw = serde_json::json!({ "type": "backup_requested" }).to_string();
        assert!(matches!(
            parse_webview_message(&raw).expect("backup request"),
            WebviewToHostMessage::BackupRequested
        ));
    }

    #[test]
    fn a_data_request_cannot_name_a_file() {
        // The trash and the backup both live on disk, and neither is reachable
        // by name from the page: every note in these messages is a `Uuid`, so
        // a path is not a value that can be spelled in one.
        for payload in [
            serde_json::json!({ "noteId": "../../etc/passwd" }),
            serde_json::json!({ "noteId": "/home/alguem/.local/share/note-it/notes/a.md" }),
            serde_json::json!({ "noteId": "a.md" }),
            serde_json::json!({ "noteId": "" }),
            serde_json::json!({ "noteId": 42 }),
        ] {
            let raw = serde_json::json!({
                "type": "restore_note_requested",
                "payload": payload,
            })
            .to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }

        for payload in [
            serde_json::json!({ "id": "../../../notes" }),
            serde_json::json!({ "id": "trash/../notes/a.md" }),
        ] {
            let raw = serde_json::json!({
                "type": "trash_note_requested",
                "payload": payload,
            })
            .to_string();
            assert!(parse_webview_message(&raw).is_err(), "accepted {raw}");
        }
    }

    #[test]
    fn the_trash_travels_to_the_page_as_identifiers_and_text() {
        let note_id = Uuid::new_v4();
        let deleted_at = chrono::DateTime::parse_from_rfc3339("2026-08-29T09:30:00Z")
            .expect("fixed instant")
            .with_timezone(&chrono::Utc);

        let encoded = serde_json::to_value(super::HostToWebviewMessage::TrashEntries {
            request_id: 3,
            entries: vec![
                crate::trash::TrashEntry {
                    note_id,
                    label: "Uma nota".to_string(),
                    snippet: "<script>alert(1)</script>".to_string(),
                    deleted_at: Some(deleted_at),
                },
                crate::trash::TrashEntry {
                    note_id: Uuid::new_v4(),
                    label: "Sem data".to_string(),
                    snippet: String::new(),
                    deleted_at: None,
                },
            ],
        })
        .expect("serialize trash entries");

        assert_eq!(encoded["type"], "trash_entries");
        assert_eq!(encoded["payload"]["requestId"], 3);
        let entries = &encoded["payload"]["entries"];
        assert_eq!(entries[0]["noteId"], note_id.to_string());
        assert_eq!(entries[0]["deletedAt"], "2026-08-29T09:30:00Z");
        // A snippet is text on the wire too; nothing escapes it into markup,
        // and the page renders it with `textContent`.
        assert_eq!(entries[0]["snippet"], "<script>alert(1)</script>");
        // An unknown date travels as null rather than as an invented one.
        assert!(entries[1]["deletedAt"].is_null());
        // No path of any kind reaches the page.
        assert!(!encoded.to_string().contains(".md"));
    }

    #[test]
    fn a_data_result_carries_the_sentence_to_show() {
        for (action, ok, message) in [
            ("backup", true, "Backup concluído."),
            (
                "backup",
                false,
                "Não foi possível criar o backup. Nada foi alterado.",
            ),
            ("restore", true, "Nota restaurada."),
            (
                "trash",
                false,
                "Não foi possível mover a nota para a lixeira.",
            ),
        ] {
            let encoded = serde_json::to_value(super::HostToWebviewMessage::DataResult {
                action: action.to_string(),
                ok,
                message: message.to_string(),
            })
            .expect("serialize data result");
            assert_eq!(encoded["type"], "data_result");
            assert_eq!(encoded["payload"]["action"], action);
            assert_eq!(encoded["payload"]["ok"], ok);
            assert_eq!(encoded["payload"]["message"], message);
        }
    }
}
