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
        #[serde(rename = "fontSize")]
        font_size: u32,
    },
    SetColor {
        color: String,
    },
    SetFontSize {
        #[serde(rename = "fontSize")]
        font_size: u32,
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
}
