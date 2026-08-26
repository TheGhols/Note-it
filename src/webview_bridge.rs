use serde::{Deserialize, Serialize};
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::WebView;

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
    CloseRequested {
        id: Uuid,
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
}

pub fn send_to_webview(webview: &WebView, message: &HostToWebviewMessage) {
    if let Ok(json_str) = serde_json::to_string(message) {
        let script = format!("window.handleHostMessage && window.handleHostMessage({json_str});");
        webview.evaluate_javascript(&script, None, None, gio::Cancellable::NONE, |_| {});
    }
}

pub fn parse_webview_message(raw_json: &str) -> Result<WebviewToHostMessage, String> {
    serde_json::from_str::<WebviewToHostMessage>(raw_json)
        .map_err(|e| format!("Failed to parse webview message: {e}"))
}
