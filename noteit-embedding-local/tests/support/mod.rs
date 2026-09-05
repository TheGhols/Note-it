//! Synthetic artifacts, built byte by byte.
//!
//! Every contract of `LocalProvider` is provable without the 489 MiB model:
//! the guarantees are about shape, identity and refusal, none of which need a
//! real vocabulary. That keeps the suite runnable on a machine — and in a CI
//! job — that has never provisioned an artifact and has no network to do it
//! with, which is the same posture the factory default has.

use std::fs;
use std::path::Path;

/// A tiny WordPiece tokenizer, valid enough for the real crate to load.
pub fn tokenizer_json(extra_token: Option<&str>) -> Vec<u8> {
    let mut vocabulary = vec![
        ("[UNK]", 0),
        ("nota", 1),
        ("chuva", 2),
        ("pressao", 3),
        ("alta", 4),
        ("sono", 5),
        ("reuniao", 6),
    ];
    if let Some(token) = extra_token {
        vocabulary.push((token, 7));
    }
    let entries: Vec<String> = vocabulary
        .iter()
        .map(|(token, id)| format!("\"{token}\": {id}"))
        .collect();
    let hash = '#';
    format!(
        r#"{{
  "version": "1.0",
  "truncation": null,
  "padding": null,
  "added_tokens": [],
  "normalizer": {{"type": "BertNormalizer", "clean_text": true, "handle_chinese_chars": true, "strip_accents": true, "lowercase": true}},
  "pre_tokenizer": {{"type": "Whitespace"}},
  "post_processor": null,
  "decoder": null,
  "model": {{
    "type": "WordPiece",
    "unk_token": "[UNK]",
    "continuing_subword_prefix": "{hash}{hash}",
    "max_input_chars_per_word": 100,
    "vocab": {{{}}}
  }}
}}"#,
        entries.join(", ")
    )
    .into_bytes()
}

/// A safetensors file holding one `f32` table, written by hand so that every
/// malformed variant below is a deliberate edit rather than a library's mercy.
pub fn safetensors(name: &str, rows: usize, dimension: usize, seed: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(rows * dimension * 4);
    for row in 0..rows {
        for column in 0..dimension {
            // Deterministic, and different per row, so two rows are never the
            // same direction by accident.
            let value = ((row * 31 + column * 7 + seed as usize) % 97) as f32 / 97.0 + 0.01;
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    let header = format!(
        r#"{{"{name}":{{"dtype":"F32","shape":[{rows},{dimension}],"data_offsets":[0,{}]}}}}"#,
        payload.len()
    );
    let mut bytes = Vec::with_capacity(8 + header.len() + payload.len());
    bytes.extend_from_slice(&(header.len() as u64).to_le_bytes());
    bytes.extend_from_slice(header.as_bytes());
    bytes.extend_from_slice(&payload);
    bytes
}

/// Writes an artifact directory and returns the two files' contents.
pub fn write_artifact(directory: &Path, weights: &[u8], tokenizer: &[u8]) {
    fs::create_dir_all(directory).expect("artifact directory");
    fs::write(directory.join("model.safetensors"), weights).expect("weights");
    fs::write(directory.join("tokenizer.json"), tokenizer).expect("tokenizer");
}
