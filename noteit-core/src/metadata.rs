//! User-authored semantic metadata carried by a note's front matter.
//!
//! Tags and properties are domain values, not YAML values. Adapters receive
//! these types and the persistence layer alone decides how they are written.

use crate::search::fold;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_TAGS: usize = 32;
pub const MAX_TAG_CHARS: usize = 64;
pub const MAX_PROPERTIES: usize = 32;
pub const MAX_PROPERTY_KEY_CHARS: usize = 64;
pub const MAX_PROPERTY_VALUE_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataError(String);

impl MetadataError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MetadataError {}

/// Comparison identity shared with body search: Unicode lowercase followed by
/// the Latin accent folding already used by Note-it search.
pub fn semantic_identity(value: &str) -> String {
    fold(value).text
}

fn has_forbidden_character(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn normalize_tag_display(raw: &str) -> Result<String, MetadataError> {
    let trimmed = raw.trim();
    let without_hash = trimmed.strip_prefix('#').unwrap_or(trimmed).trim();
    if without_hash.is_empty() {
        return Err(MetadataError::new("a tag não pode ser vazia"));
    }
    if has_forbidden_character(without_hash) {
        return Err(MetadataError::new(
            "a tag não pode conter quebras de linha ou caracteres de controle",
        ));
    }
    if without_hash.starts_with('#') {
        return Err(MetadataError::new(
            "a tag pode ter no máximo um # inicial de conveniência",
        ));
    }
    if without_hash.chars().count() > MAX_TAG_CHARS {
        return Err(MetadataError::new(format!(
            "a tag excede o limite de {MAX_TAG_CHARS} caracteres"
        )));
    }
    Ok(without_hash.to_string())
}

/// An ordered, deduplicated set of human-readable tag spellings.
///
/// The first spelling wins (`Urgência` is retained when `urgencia` follows),
/// while identity is case- and accent-insensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteTags(Vec<String>);

impl NoteTags {
    pub fn try_new(tags: impl IntoIterator<Item = String>) -> Result<Self, MetadataError> {
        let mut identities = BTreeSet::new();
        let mut accepted = Vec::new();
        for raw in tags {
            let display = normalize_tag_display(&raw)?;
            let identity = semantic_identity(&display);
            if identities.insert(identity) {
                if accepted.len() == MAX_TAGS {
                    return Err(MetadataError::new(format!(
                        "a nota aceita no máximo {MAX_TAGS} tags"
                    )));
                }
                accepted.push(display);
            }
        }
        Ok(Self(accepted))
    }

    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for NoteTags {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for tag in &self.0 {
            sequence.serialize_element(tag)?;
        }
        sequence.end()
    }
}

impl<'de> Deserialize<'de> for NoteTags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TagsVisitor;

        impl<'de> Visitor<'de> for TagsVisitor {
            type Value = NoteTags;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("uma lista de tags textuais")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut tags = Vec::new();
                while let Some(tag) = sequence.next_element::<String>()? {
                    tags.push(tag);
                }
                NoteTags::try_new(tags).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_seq(TagsVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteProperty {
    pub key: String,
    pub value: String,
}

fn normalize_property(property: NoteProperty) -> Result<NoteProperty, MetadataError> {
    let key = property.key.trim();
    if key.is_empty() {
        return Err(MetadataError::new(
            "a chave da propriedade não pode ser vazia",
        ));
    }
    if has_forbidden_character(key) {
        return Err(MetadataError::new(
            "a chave da propriedade não pode conter quebras de linha ou caracteres de controle",
        ));
    }
    if key.chars().count() > MAX_PROPERTY_KEY_CHARS {
        return Err(MetadataError::new(format!(
            "a chave da propriedade excede o limite de {MAX_PROPERTY_KEY_CHARS} caracteres"
        )));
    }
    if has_forbidden_character(&property.value) {
        return Err(MetadataError::new(
            "o valor da propriedade não pode conter quebras de linha ou caracteres de controle",
        ));
    }
    if property.value.chars().count() > MAX_PROPERTY_VALUE_CHARS {
        return Err(MetadataError::new(format!(
            "o valor da propriedade excede o limite de {MAX_PROPERTY_VALUE_CHARS} caracteres"
        )));
    }
    Ok(NoteProperty {
        key: key.to_string(),
        value: property.value,
    })
}

/// Text properties sorted by semantic key identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteProperties(Vec<NoteProperty>);

impl NoteProperties {
    pub fn try_new(
        properties: impl IntoIterator<Item = NoteProperty>,
    ) -> Result<Self, MetadataError> {
        let mut by_identity = BTreeMap::new();
        for property in properties {
            let property = normalize_property(property)?;
            let identity = semantic_identity(&property.key);
            if by_identity.insert(identity, property).is_some() {
                return Err(MetadataError::new(
                    "a nota não pode ter chaves de propriedade semanticamente duplicadas",
                ));
            }
            if by_identity.len() > MAX_PROPERTIES {
                return Err(MetadataError::new(format!(
                    "a nota aceita no máximo {MAX_PROPERTIES} propriedades"
                )));
            }
        }
        Ok(Self(by_identity.into_values().collect()))
    }

    pub fn as_slice(&self) -> &[NoteProperty] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for NoteProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut mapping = serializer.serialize_map(Some(self.0.len()))?;
        for property in &self.0 {
            mapping.serialize_entry(&property.key, &property.value)?;
        }
        mapping.end()
    }
}

impl<'de> Deserialize<'de> for NoteProperties {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PropertiesVisitor;

        impl<'de> Visitor<'de> for PropertiesVisitor {
            type Value = NoteProperties;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("um mapa de chaves e valores textuais")
            }

            fn visit_map<A>(self, mut mapping: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut properties = Vec::new();
                while let Some((key, value)) = mapping.next_entry::<String, String>()? {
                    properties.push(NoteProperty { key, value });
                }
                NoteProperties::try_new(properties).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_map(PropertiesVisitor)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteMetadata {
    #[serde(default)]
    pub tags: NoteTags,
    #[serde(default)]
    pub properties: NoteProperties,
}

impl NoteMetadata {
    pub fn try_new(
        tags: impl IntoIterator<Item = String>,
        properties: impl IntoIterator<Item = NoteProperty>,
    ) -> Result<Self, MetadataError> {
        Ok(Self {
            tags: NoteTags::try_new(tags)?,
            properties: NoteProperties::try_new(properties)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCatalogEntry {
    pub tag: String,
    pub note_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyKeyCatalogEntry {
    pub key: String,
    pub note_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataCatalog {
    pub tags: Vec<TagCatalogEntry>,
    pub property_keys: Vec<PropertyKeyCatalogEntry>,
}

impl MetadataCatalog {
    pub fn tag_suggestions(&self, query: &str) -> Vec<String> {
        matching_values(self.tags.iter().map(|entry| entry.tag.as_str()), query)
    }

    pub fn property_key_suggestions(&self, query: &str) -> Vec<String> {
        matching_values(
            self.property_keys.iter().map(|entry| entry.key.as_str()),
            query,
        )
    }
}

fn matching_values<'a>(values: impl Iterator<Item = &'a str>, query: &str) -> Vec<String> {
    let query = semantic_identity(query.trim());
    values
        .filter(|value| query.is_empty() || semantic_identity(value).contains(&query))
        .take(8)
        .map(str::to_string)
        .collect()
}

/// Stable FNV-1a bucket derived from the Core identity. The UI supplies only
/// the size of its reviewed palette; user text never becomes a class or style.
pub fn tag_colour_bucket(tag: &str, palette_size: usize) -> usize {
    if palette_size == 0 {
        return 0;
    }
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in semantic_identity(tag).as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash % palette_size as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_keep_first_spelling_and_dedupe_case_and_portuguese_accents() {
        let tags = NoteTags::try_new([
            " #Medicina ".to_string(),
            "MEDICINA".to_string(),
            "Urgência".to_string(),
            "urgencia".to_string(),
            "Clínica Médica".to_string(),
        ])
        .expect("valid tags");
        assert_eq!(tags.as_slice(), ["Medicina", "Urgência", "Clínica Médica"]);
    }

    #[test]
    fn metadata_limits_are_rejected_without_truncation() {
        assert!(NoteTags::try_new(["x".repeat(MAX_TAG_CHARS + 1)]).is_err());
        assert!(NoteTags::try_new((0..=MAX_TAGS).map(|index| format!("tag {index}"))).is_err());
        assert!(NoteTags::try_new(["linha\nnova".into()]).is_err());
        assert!(NoteProperties::try_new([NoteProperty {
            key: "x".repeat(MAX_PROPERTY_KEY_CHARS + 1),
            value: "valor".into(),
        }])
        .is_err());
        assert!(NoteProperties::try_new([NoteProperty {
            key: "chave".into(),
            value: "x".repeat(MAX_PROPERTY_VALUE_CHARS + 1),
        }])
        .is_err());
        assert!(
            NoteProperties::try_new((0..=MAX_PROPERTIES).map(|index| NoteProperty {
                key: format!("campo {index}"),
                value: "valor".into(),
            }))
            .is_err()
        );
        assert!(NoteProperties::try_new([NoteProperty {
            key: "campo".into(),
            value: "linha\nnova".into(),
        }])
        .is_err());
    }

    #[test]
    fn properties_are_trimmed_sorted_and_reject_semantic_duplicates() {
        let properties = NoteProperties::try_new([
            NoteProperty {
                key: " tipo ".into(),
                value: "estudo".into(),
            },
            NoteProperty {
                key: "Disciplina".into(),
                value: "cardiologia".into(),
            },
        ])
        .expect("valid properties");
        assert_eq!(properties.as_slice()[0].key, "Disciplina");
        assert_eq!(properties.as_slice()[1].key, "tipo");

        assert!(NoteProperties::try_new([
            NoteProperty {
                key: "Status".into(),
                value: "a".into()
            },
            NoteProperty {
                key: "status".into(),
                value: "b".into()
            },
        ])
        .is_err());
        assert!(NoteProperties::try_new([
            NoteProperty {
                key: "Situação".into(),
                value: "a".into()
            },
            NoteProperty {
                key: "situacao".into(),
                value: "b".into()
            },
        ])
        .is_err());
    }

    #[test]
    fn colour_uses_semantic_identity_and_is_stable() {
        assert_eq!(
            tag_colour_bucket("Medicina", 7),
            tag_colour_bucket("medicina", 7)
        );
        assert_eq!(
            tag_colour_bucket("Urgência", 7),
            tag_colour_bucket("urgencia", 7)
        );
        assert_eq!(tag_colour_bucket("Hotel", 7), tag_colour_bucket("Hotel", 7));
    }

    #[test]
    fn catalog_suggestions_match_case_and_accents_with_the_shared_policy() {
        let catalog = MetadataCatalog {
            tags: vec![TagCatalogEntry {
                tag: "Urgência".into(),
                note_count: 3,
            }],
            property_keys: vec![PropertyKeyCatalogEntry {
                key: "Situação".into(),
                note_count: 2,
            }],
        };
        assert_eq!(catalog.tag_suggestions("URGENCIA"), ["Urgência"]);
        assert_eq!(catalog.property_key_suggestions("situacao"), ["Situação"]);
    }
}
