//! Tests for R-007: Unicode bidirectional control characters neutralization in human terminal presentation.
//!
//! Verifies that Trojan Source / bidi control characters are neutralized in human terminal presentation,
//! while stored note bytes and machine JSON output remain completely faithful, and legitimate RTL
//! (Arabic, Hebrew), accents, and emojis are preserved intact.

use noteit_cli::output::sanitize_for_terminal;
use noteit_core::model::NoteDocument;
use noteit_core::{NoteItCore, StorageManager, StorePaths};
use std::fs;
use tempfile::tempdir;

#[test]
fn r007_1_bidi_override_in_terminal_presentation_neutralized() {
    // Trojan source payload: attempting to invert extension from .sh to .jpg
    let hostile_title = "malware\u{202E}gpj.sh";
    let sanitized = sanitize_for_terminal(hostile_title);

    // Must not contain raw bidi override
    assert!(
        !sanitized.contains('\u{202E}'),
        "Raw RLO character U+202E must not be emitted to terminal"
    );
    // Must contain explicit visible neutralization
    assert!(
        sanitized.contains("[U+202E]"),
        "Sanitized output must explicitly show [U+202E], got: {sanitized}"
    );
    assert_eq!(sanitized, "malware[U+202E]gpj.sh");
}

#[test]
fn r007_2_all_bidi_controls_neutralized() {
    let controls = [
        ('\u{202A}', "202A"), // LRE
        ('\u{202B}', "202B"), // RLE
        ('\u{202C}', "202C"), // PDF
        ('\u{202D}', "202D"), // LRO
        ('\u{202E}', "202E"), // RLO
        ('\u{2066}', "2066"), // LRI
        ('\u{2067}', "2067"), // RLI
        ('\u{2068}', "2068"), // FSI
        ('\u{2069}', "2069"), // PDI
        ('\u{200E}', "200E"), // LRM
        ('\u{200F}', "200F"), // RLM
        ('\u{061C}', "061C"), // ALM
    ];

    for (c, code) in controls {
        let input = format!("prefix{c}suffix");
        let sanitized = sanitize_for_terminal(&input);
        assert!(!sanitized.contains(c), "Control U+{code} must be stripped/escaped");
        assert!(
            sanitized.contains(&format!("[U+{code}]")),
            "Expected [U+{code}] in '{sanitized}'"
        );
    }
}

#[test]
fn r007_3_legitimate_rtl_arabic_hebrew_and_accents_preserved() {
    // Arabic text (natural RTL, without override controls)
    let arabic = "مرحبا بالعالم - ملاحظة جديدة";
    assert_eq!(sanitize_for_terminal(arabic), arabic);

    // Hebrew text (natural RTL)
    let hebrew = "שלום עולם - פתק חדש";
    assert_eq!(sanitize_for_terminal(hebrew), hebrew);

    // Portuguese accented text and emojis
    let pt = "Acentuação em títulos: Coração, Biópsia & Ações 🚀🎉";
    assert_eq!(sanitize_for_terminal(pt), pt);
}

#[test]
fn r007_4_stored_note_bytes_remain_unaltered_by_sanitization() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path().to_path_buf();
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        root.join("runtime/note-it"),
    );

    let storage = StorageManager::from_paths(paths).expect("open storage");
    let core = NoteItCore::from_storage(storage);
    core.storage().ensure_directories().expect("dirs");

    let raw_payload = "Título com trojan: \u{202E}txt.sh\nConteúdo com \u{200F}marca.";
    let mut doc = NoteDocument::new_empty();
    doc.content = raw_payload.to_string();

    core.storage().save_note_atomic(&doc).expect("save note");

    let note_path = core.storage().note_path(&doc.metadata.id);
    let disk_bytes = fs::read_to_string(&note_path).expect("read note from disk");

    // The stored markdown must contain the EXACT raw characters intact!
    assert!(
        disk_bytes.contains('\u{202E}'),
        "Raw disk bytes must preserve exact note content without mutation"
    );
    assert!(
        disk_bytes.contains('\u{200F}'),
        "Raw disk bytes must preserve exact note content without mutation"
    );
}
