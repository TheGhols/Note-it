use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFrontMatter {
    #[serde(default = "default_version")]
    pub version: u32,
    pub id: Uuid,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_version() -> u32 {
    1
}

fn default_color() -> String {
    "yellow".to_string()
}

fn default_font_size() -> u32 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteFrontMatterWrapper {
    pub note_it: NoteFrontMatter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDocument {
    pub metadata: NoteFrontMatter,
    pub content: String,
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
                font_size: 15,
                created_at: now,
                updated_at: now,
            },
            content: String::new(),
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
                font_size: 15,
                created_at: now,
                updated_at: now,
            },
            content: String::new(),
        }
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("---") {
            // No front matter present, create default metadata
            let doc = Self::new_empty();
            return Ok(Self {
                metadata: doc.metadata,
                content: raw.to_string(),
            });
        }

        let rest = &trimmed[3..];
        let end_idx = rest.find("\n---").ok_or_else(|| {
            "Invalid markdown front matter: missing closing delimiter '---'".to_string()
        })?;

        let yaml_str = &rest[..end_idx];
        let content_start = end_idx + 4; // Skip \n---
        let content = rest[content_start..].trim_start_matches(['\r', '\n']);

        let wrapper: NoteFrontMatterWrapper = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Failed to parse YAML front matter: {e}"))?;

        Ok(Self {
            metadata: wrapper.note_it,
            content: content.to_string(),
        })
    }

    pub fn serialize(&self) -> Result<String, String> {
        let wrapper = NoteFrontMatterWrapper {
            note_it: self.metadata.clone(),
        };

        let yaml_str = serde_yaml::to_string(&wrapper)
            .map_err(|e| format!("Failed to serialize YAML front matter: {e}"))?;

        Ok(format!("---\n{}---\n\n{}", yaml_str, self.content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_parse_without_front_matter() {
        let raw = "Just plain text without header";
        let parsed = NoteDocument::parse(raw).expect("Should fallback cleanly");
        assert_eq!(parsed.content, raw);
    }
}
