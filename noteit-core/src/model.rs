use crate::metadata::{NoteMetadata, NoteProperties, NoteTags};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Paper patterns a note can carry, in the order the menu offers them.
///
/// Stored as a plain string like `color` rather than as a serde enum: a value
/// written by a newer version, or by hand, then degrades to the default
/// instead of failing the parse and taking the whole note down with it.
pub const PAPER_TYPES: &[&str] = &["blank", "lined", "dotted", "grid-small", "grid-large"];
pub const DEFAULT_PAPER_TYPE: &str = "blank";

/// How strongly the pattern is drawn. Kept even for `blank`, where it simply
/// has nothing to act on, so switching paper back and forth never loses it.
pub const PAPER_INTENSITIES: &[&str] = &["subtle", "normal", "strong"];
pub const DEFAULT_PAPER_INTENSITY: &str = "normal";

/// Resolves a stored paper pattern to the supported set, falling back to the
/// default so an unknown value can never leave a note unrenderable.
pub fn paper_type_name(value: &str) -> &'static str {
    PAPER_TYPES
        .iter()
        .find(|name| **name == value)
        .copied()
        .unwrap_or(DEFAULT_PAPER_TYPE)
}

/// Same contract as [`paper_type_name`], for the pattern intensity.
pub fn paper_intensity_name(value: &str) -> &'static str {
    PAPER_INTENSITIES
        .iter()
        .find(|name| **name == value)
        .copied()
        .unwrap_or(DEFAULT_PAPER_INTENSITY)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFrontMatter {
    #[serde(default = "default_version")]
    pub version: u32,
    pub id: Uuid,
    #[serde(default = "default_color")]
    pub color: String,
    /// Background pattern of the paper: `blank`, `lined`, `dotted`,
    /// `grid-small` or `grid-large`.
    #[serde(default = "default_paper_type")]
    pub paper_type: String,
    /// How strongly that pattern is drawn: `subtle`, `normal` or `strong`.
    #[serde(default = "default_paper_intensity")]
    pub paper_intensity: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    /// Absent only for notes whose front matter predates or omits the field.
    /// A missing timestamp is reported as unknown, never replaced by a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_version() -> u32 {
    1
}

fn default_color() -> String {
    "yellow".to_string()
}

fn default_paper_type() -> String {
    DEFAULT_PAPER_TYPE.to_string()
}

fn default_paper_intensity() -> String {
    DEFAULT_PAPER_INTENSITY.to_string()
}

fn default_font_size() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFrontMatterWrapper {
    pub note_it: NoteFrontMatter,
    #[serde(default, skip_serializing_if = "NoteTags::is_empty")]
    pub tags: NoteTags,
    #[serde(default, skip_serializing_if = "NoteProperties::is_empty")]
    pub properties: NoteProperties,
    /// Top-level YAML owned by other tools or future Note-it versions.
    #[serde(default, flatten)]
    unknown: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDocument {
    pub metadata: NoteFrontMatter,
    pub user_metadata: NoteMetadata,
    pub content: String,
    unknown_front_matter: BTreeMap<String, serde_yaml::Value>,
}

impl NoteDocument {
    pub fn new_empty() -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4();
        Self {
            metadata: NoteFrontMatter {
                version: 1,
                id,
                color: "yellow".to_string(),
                paper_type: default_paper_type(),
                paper_intensity: default_paper_intensity(),
                font_size: 15,
                created_at: Some(now),
                updated_at: Some(now),
            },
            user_metadata: NoteMetadata::default(),
            content: String::new(),
            unknown_front_matter: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn new_with_id(id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            metadata: NoteFrontMatter {
                version: 1,
                id,
                color: "yellow".to_string(),
                paper_type: default_paper_type(),
                paper_intensity: default_paper_intensity(),
                font_size: 15,
                created_at: Some(now),
                updated_at: Some(now),
            },
            user_metadata: NoteMetadata::default(),
            content: String::new(),
            unknown_front_matter: BTreeMap::new(),
        }
    }

    /// Records a content edit. Appearance-only metadata (paper colour, paper
    /// pattern, pattern intensity, font size) deliberately does not go through
    /// here: `updated_at` tracks the last change to the note's text, not to
    /// how it is displayed.
    pub fn touch_content_modified(&mut self) {
        self.metadata.updated_at = Some(Utc::now());
    }

    /// The note as Note-it holds it, without the blank lines a file or a
    /// serializer ends with.
    ///
    /// Two things put newlines on the end of a note and neither of them is
    /// content. Every editor terminates a file with one, and Markdown gives a
    /// trailing blank line no meaning. The page's own serializer terminates a
    /// document that ends in a block — a list, a callout, a code block — with
    /// a blank line, while one ending in a paragraph gets none.
    ///
    /// So the same note has several equally valid spellings, and comparing
    /// them literally made opening a note look like editing it: a `.md` written
    /// elsewhere, or any note ending in a list, was rewritten and had its
    /// modification date moved by nothing more than being opened. Everything
    /// that decides whether a note changed compares this form, and this is the
    /// form that gets stored.
    ///
    /// Only line terminators are removed. Trailing spaces are Markdown's hard
    /// line break and are content.
    pub fn canonical_content(content: &str) -> &str {
        content.trim_end_matches(['\n', '\r'])
    }

    /// Splits a stored file into its front matter and the note itself.
    ///
    /// One definition of "where the note starts", because two would eventually
    /// disagree: [`parse`](Self::parse) reads the metadata through it, and
    /// search reads the body through it without paying for the YAML. `None`
    /// means there is no front matter and the whole file is the note.
    pub fn split_front_matter(raw: &str) -> (Option<&str>, &str) {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("---") {
            return (None, raw);
        }

        let rest = &trimmed[3..];
        let Some(end) = rest.find("\n---") else {
            return (None, raw);
        };

        // Skip the closing `\n---` and the line break that follows it.
        let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
        (Some(&rest[..end]), body)
    }

    /// The note's own text, with any front matter and trailing newlines gone.
    ///
    /// This is what a reader wrote and therefore what search looks at: the
    /// stored metadata is Note-it's bookkeeping, not something anyone typed.
    pub fn body_of(raw: &str) -> &str {
        Self::canonical_content(Self::split_front_matter(raw).1)
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        let (front_matter, content) = Self::split_front_matter(raw);

        let Some(yaml_str) = front_matter else {
            if raw.trim_start().starts_with("---") {
                return Err(
                    "Invalid markdown front matter: missing closing delimiter '---'".to_string(),
                );
            }
            // No front matter present, create default metadata
            let doc = Self::new_empty();
            return Ok(Self {
                metadata: doc.metadata,
                user_metadata: doc.user_metadata,
                content: Self::canonical_content(raw).to_string(),
                unknown_front_matter: BTreeMap::new(),
            });
        };

        let wrapper: NoteFrontMatterWrapper = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Failed to parse YAML front matter: {e}"))?;

        Ok(Self {
            metadata: wrapper.note_it,
            user_metadata: NoteMetadata {
                tags: wrapper.tags,
                properties: wrapper.properties,
            },
            content: Self::canonical_content(content).to_string(),
            unknown_front_matter: wrapper.unknown,
        })
    }

    pub fn serialize(&self) -> Result<String, String> {
        let wrapper = NoteFrontMatterWrapper {
            note_it: self.metadata.clone(),
            tags: self.user_metadata.tags.clone(),
            properties: self.user_metadata.properties.clone(),
            unknown: self.unknown_front_matter.clone(),
        };

        let yaml_str = serde_yaml::to_string(&wrapper)
            .map_err(|e| format!("Failed to serialize YAML front matter: {e}"))?;

        // The note is stored terminated, the way every other tool writes a
        // file; `parse` takes that terminator back off, so the pair round-trips
        // a note unchanged however many times it is written and read.
        Ok(format!("---\n{}---\n\n{}\n", yaml_str, self.content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{NoteMetadata, NoteProperty};

    #[test]
    fn test_note_round_trip() {
        let original_doc = NoteDocument::new_empty();
        let mut doc = original_doc.clone();
        doc.content =
            "# My First Note\n\n- [ ] Task 1\n- [x] Task 2\n\n<u>Underlined</u> text.".to_string();

        let serialized = doc.serialize().expect("Serialization should succeed");
        let parsed = NoteDocument::parse(&serialized).expect("Parsing should succeed");

        assert_eq!(parsed.metadata.id, doc.metadata.id);
        assert_eq!(parsed.metadata.color, "yellow");
        assert_eq!(parsed.metadata.font_size, 15);
        assert_eq!(parsed.content, doc.content);
        assert_eq!(parsed.metadata.created_at, doc.metadata.created_at);
        assert_eq!(parsed.metadata.updated_at, doc.metadata.updated_at);
    }

    #[test]
    fn legacy_note_without_timestamps_still_loads() {
        let legacy = concat!(
            "---\n",
            "note_it:\n",
            "  version: 1\n",
            "  id: 00000000-0000-0000-0000-000000000042\n",
            "  color: blue\n",
            "  font_size: 15\n",
            "---\n\n",
            "# Nota antiga\n",
        );

        let parsed = NoteDocument::parse(legacy).expect("legacy note must keep opening");
        assert_eq!(
            parsed.metadata.id,
            Uuid::parse_str("00000000-0000-0000-0000-000000000042").unwrap()
        );
        assert_eq!(parsed.metadata.color, "blue");
        // A note written before the paper existed opens as plain paper.
        assert_eq!(parsed.metadata.paper_type, DEFAULT_PAPER_TYPE);
        assert_eq!(parsed.metadata.paper_intensity, DEFAULT_PAPER_INTENSITY);
        // No invented dates: unknown stays unknown.
        assert_eq!(parsed.metadata.created_at, None);
        assert_eq!(parsed.metadata.updated_at, None);

        // Re-serializing must not fabricate a creation date either.
        let serialized = parsed.serialize().expect("serialize legacy note");
        assert!(!serialized.contains("created_at"));
        assert!(!serialized.contains("updated_at"));
    }

    #[test]
    fn content_edits_move_updated_at_but_never_created_at() {
        let mut doc = NoteDocument::new_empty();
        let created_at = doc.metadata.created_at;
        let original_updated_at = doc.metadata.updated_at;

        doc.content = "conteúdo novo".to_string();
        doc.touch_content_modified();

        assert_eq!(doc.metadata.created_at, created_at);
        assert!(doc.metadata.updated_at >= original_updated_at);
        assert!(doc.metadata.updated_at.is_some());
    }

    #[test]
    fn every_paper_type_and_intensity_survives_a_round_trip() {
        for paper_type in PAPER_TYPES {
            for intensity in PAPER_INTENSITIES {
                let mut doc = NoteDocument::new_empty();
                // No trailing newline: the terminator belongs to the file,
                // and `parse` strips it back off. See
                // `the_file_terminator_is_not_part_of_the_note`.
                doc.content = "# Conteúdo\n\n- [ ] Tarefa".to_string();
                doc.metadata.paper_type = (*paper_type).to_string();
                doc.metadata.paper_intensity = (*intensity).to_string();

                let serialized = doc.serialize().expect("serialize");
                let parsed = NoteDocument::parse(&serialized).expect("parse");

                assert_eq!(parsed.metadata.paper_type, *paper_type);
                assert_eq!(parsed.metadata.paper_intensity, *intensity);
                // The pattern is note metadata, never document decoration.
                assert_eq!(parsed.content, doc.content);
                assert!(!parsed.content.contains("paper"));
            }
        }
    }

    #[test]
    fn an_unknown_paper_value_degrades_to_the_default() {
        for unknown in ["", "quadriculado", "GRID-SMALL", "lined ", "canvas"] {
            assert_eq!(paper_type_name(unknown), DEFAULT_PAPER_TYPE);
            assert_eq!(paper_intensity_name(unknown), DEFAULT_PAPER_INTENSITY);
        }
        for name in PAPER_TYPES {
            assert_eq!(paper_type_name(name), *name);
        }
        for name in PAPER_INTENSITIES {
            assert_eq!(paper_intensity_name(name), *name);
        }
    }

    #[test]
    fn a_note_carrying_an_unknown_paper_still_opens() {
        // Hand-edited front matter must not cost the user the note.
        let raw = concat!(
            "---\n",
            "note_it:\n",
            "  version: 1\n",
            "  id: 00000000-0000-0000-0000-000000000077\n",
            "  color: black\n",
            "  paper_type: hexagonal\n",
            "  paper_intensity: violento\n",
            "  font_size: 15\n",
            "---\n\n",
            "texto\n",
        );

        let parsed = NoteDocument::parse(raw).expect("unknown paper must not lose the note");
        assert_eq!(
            paper_type_name(&parsed.metadata.paper_type),
            DEFAULT_PAPER_TYPE
        );
        assert_eq!(
            paper_intensity_name(&parsed.metadata.paper_intensity),
            DEFAULT_PAPER_INTENSITY
        );
        assert_eq!(parsed.content, "texto");
    }

    #[test]
    fn a_new_note_starts_on_plain_paper_at_normal_intensity() {
        let doc = NoteDocument::new_empty();
        assert_eq!(doc.metadata.paper_type, "blank");
        assert_eq!(doc.metadata.paper_intensity, "normal");
    }

    #[test]
    fn changing_the_paper_never_moves_the_modification_date() {
        let mut doc = NoteDocument::new_empty();
        doc.content = "conteúdo".to_string();
        let updated_at = doc.metadata.updated_at;

        doc.metadata.paper_type = "grid-large".to_string();
        doc.metadata.paper_intensity = "strong".to_string();

        // Nothing here goes through `touch_content_modified`.
        assert_eq!(doc.metadata.updated_at, updated_at);
    }

    #[test]
    fn legacy_metadata_defaults_empty_and_is_not_written_back_as_empty_fields() {
        let raw = concat!(
            "---\n",
            "note_it:\n",
            "  id: 00000000-0000-4000-8000-000000000042\n",
            "---\n\n",
            "texto\n",
        );
        let parsed = NoteDocument::parse(raw).expect("legacy note");
        assert!(parsed.user_metadata.tags.is_empty());
        assert!(parsed.user_metadata.properties.is_empty());

        let serialized = parsed.serialize().expect("serialize");
        assert!(!serialized.contains("\ntags:"));
        assert!(!serialized.contains("\nproperties:"));
    }

    #[test]
    fn tags_and_properties_round_trip_together_with_unicode() {
        let mut doc = NoteDocument::new_empty();
        doc.content = "# Choque distributivo".into();
        doc.user_metadata = NoteMetadata::try_new(
            [
                "Medicina".into(),
                "Urgência".into(),
                "Clínica Médica".into(),
            ],
            [
                NoteProperty {
                    key: "tipo".into(),
                    value: "estudo".into(),
                },
                NoteProperty {
                    key: "fonte".into(),
                    value: "Harrison".into(),
                },
            ],
        )
        .expect("metadata");

        let serialized = doc.serialize().expect("serialize");
        let parsed = NoteDocument::parse(&serialized).expect("parse");
        assert_eq!(parsed.user_metadata, doc.user_metadata);
        assert_eq!(parsed.content, doc.content);
    }

    #[test]
    fn tags_round_trip_without_inventing_properties() {
        let mut doc = NoteDocument::new_empty();
        doc.user_metadata =
            NoteMetadata::try_new(["Saúde".into(), "Clínica Médica".into()], []).expect("tags");

        let serialized = doc.serialize().expect("serialize");
        let parsed = NoteDocument::parse(&serialized).expect("parse");
        assert_eq!(
            parsed.user_metadata.tags.as_slice(),
            ["Saúde", "Clínica Médica"]
        );
        assert!(parsed.user_metadata.properties.is_empty());
        assert!(!serialized.contains("\nproperties:"));
    }

    #[test]
    fn properties_round_trip_without_inventing_tags() {
        let mut doc = NoteDocument::new_empty();
        doc.user_metadata = NoteMetadata::try_new(
            [],
            [NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            }],
        )
        .expect("properties");

        let serialized = doc.serialize().expect("serialize");
        let parsed = NoteDocument::parse(&serialized).expect("parse");
        assert!(parsed.user_metadata.tags.is_empty());
        assert_eq!(
            parsed.user_metadata.properties.as_slice(),
            [NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            }]
        );
        assert!(!serialized.contains("\ntags:"));
    }

    #[test]
    fn unknown_top_level_yaml_survives_a_real_reserialization() {
        let raw = concat!(
            "---\n",
            "note_it:\n",
            "  id: 00000000-0000-4000-8000-000000000043\n",
            "future_tool:\n",
            "  enabled: true\n",
            "  nested:\n",
            "    - um\n",
            "    - dois\n",
            "external_number: 42\n",
            "---\n\n",
            "texto\n",
        );
        let mut doc = NoteDocument::parse(raw).expect("parse unknown YAML");
        doc.user_metadata = NoteMetadata::try_new(["Projeto".into()], []).expect("tag");
        let serialized = doc.serialize().expect("serialize");
        let reparsed: serde_yaml::Value = serde_yaml::from_str(
            NoteDocument::split_front_matter(&serialized)
                .0
                .expect("front matter"),
        )
        .expect("yaml");
        assert_eq!(reparsed["future_tool"]["enabled"], true);
        assert_eq!(reparsed["future_tool"]["nested"][1], "dois");
        assert_eq!(reparsed["external_number"], 42);
    }

    #[test]
    fn semantic_metadata_never_moves_created_or_updated_at() {
        let mut doc = NoteDocument::new_empty();
        let created_at = doc.metadata.created_at;
        let updated_at = doc.metadata.updated_at;
        doc.user_metadata = NoteMetadata::try_new(
            ["Saúde".into()],
            [NoteProperty {
                key: "status".into(),
                value: "revisando".into(),
            }],
        )
        .expect("metadata");
        assert_eq!(doc.metadata.created_at, created_at);
        assert_eq!(doc.metadata.updated_at, updated_at);
    }

    #[test]
    fn content_and_appearance_edits_preserve_semantic_metadata() {
        let mut doc = NoteDocument::new_empty();
        doc.user_metadata = NoteMetadata::try_new(
            ["PBL".into()],
            [NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            }],
        )
        .expect("metadata");
        let expected = doc.user_metadata.clone();

        doc.content = "conteúdo alterado".into();
        doc.touch_content_modified();
        doc.metadata.color = "black".into();
        doc.metadata.paper_type = "lined".into();
        doc.metadata.font_size = 17;

        let parsed = NoteDocument::parse(&doc.serialize().expect("serialize")).expect("parse");
        assert_eq!(parsed.user_metadata, expected);
    }

    #[test]
    fn the_file_terminator_is_not_part_of_the_note() {
        // 3.5R. A `.md` written by another editor ends with a newline. That
        // terminator belongs to the file, not to the note: the editor
        // serialises the same document back without it, and treating the two
        // as different content made a plain open-and-close look like an edit.
        let with_newline = concat!(
            "---\n",
            "note_it:\n",
            "  version: 1\n",
            "  id: 00000000-0000-0000-0000-000000000042\n",
            "  color: yellow\n",
            "  font_size: 15\n",
            "---\n\n",
            "texto\n",
        );
        let without_newline = with_newline.trim_end_matches('\n');

        assert_eq!(
            NoteDocument::parse(with_newline).expect("parse").content,
            NoteDocument::parse(without_newline).expect("parse").content,
        );
        assert_eq!(
            NoteDocument::parse(with_newline).expect("parse").content,
            "texto"
        );

        // Blank lines inside the note are content and stay untouched; only the
        // terminator at the very end goes.
        let multi = with_newline.replace("texto\n", "um\n\ndois\n\n\n");
        assert_eq!(
            NoteDocument::parse(&multi).expect("parse").content,
            "um\n\ndois"
        );
    }

    #[test]
    fn a_stored_note_ends_with_exactly_one_newline() {
        // Serializing terminates the file the way every other tool expects,
        // and parsing takes that terminator straight back off, so a note
        // survives any number of round trips through both unchanged.
        let mut doc = NoteDocument::new_empty();
        doc.content = "# Título\n\nCorpo".to_string();

        let serialized = doc.serialize().expect("serialize");
        assert!(serialized.ends_with("Corpo\n"));
        assert!(!serialized.ends_with("Corpo\n\n"));

        let reparsed = NoteDocument::parse(&serialized).expect("parse");
        assert_eq!(reparsed.content, doc.content);
        assert_eq!(reparsed.serialize().expect("re-serialize"), serialized);
    }

    #[test]
    fn a_trailing_hard_line_break_keeps_its_spaces() {
        // Only newlines are stripped. Two trailing spaces are Markdown's hard
        // line break and are content.
        let doc = NoteDocument::parse(concat!(
            "---\n",
            "note_it:\n",
            "  version: 1\n",
            "  id: 00000000-0000-0000-0000-000000000043\n",
            "  color: yellow\n",
            "  font_size: 15\n",
            "---\n\n",
            "linha  \n",
        ))
        .expect("parse");
        assert_eq!(doc.content, "linha  ");
    }

    #[test]
    fn test_parse_without_front_matter() {
        let raw = "Just plain text without header";
        let parsed = NoteDocument::parse(raw).expect("Should fallback cleanly");
        assert_eq!(parsed.content, raw);
    }
}
