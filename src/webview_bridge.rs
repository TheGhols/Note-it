use crate::search::SearchResult;
use crate::trash::TrashEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::WebView;

const MAX_GEOMETRY_DELTA: f64 = 100_000.0;

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
