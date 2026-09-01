//! Typed domain filters and selectors for note querying and addressing.

use crate::metadata::{semantic_identity, NoteMetadata};
use uuid::Uuid;

/// Typed filter for querying notes by user tags and properties with AND semantics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteFilter {
    pub tags: Vec<String>,
    pub properties: Vec<(String, String)>,
}

impl NoteFilter {
    pub fn new(tags: Vec<String>, properties: Vec<(String, String)>) -> Self {
        Self { tags, properties }
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.properties.is_empty()
    }

    /// Evaluates whether a note's metadata satisfies all tag and property constraints.
    pub fn matches(&self, metadata: &NoteMetadata) -> bool {
        // Tag constraints: all required tags must match using semantic identity (case & accent folded)
        for req_tag in &self.tags {
            let req_id = semantic_identity(req_tag.trim());
            if req_id.is_empty() {
                continue;
            }
            let found = metadata
                .tags
                .as_slice()
                .iter()
                .any(|t| semantic_identity(t) == req_id);
            if !found {
                return false;
            }
        }

        // Property constraints: all required properties must match using semantic identity
        for (req_key, req_val) in &self.properties {
            let req_key_id = semantic_identity(req_key.trim());
            if req_key_id.is_empty() {
                continue;
            }
            let req_val_id = semantic_identity(req_val.trim());

            let found = metadata.properties.as_slice().iter().any(|p| {
                semantic_identity(&p.key) == req_key_id && semantic_identity(&p.value) == req_val_id
            });
            if !found {
                return false;
            }
        }

        true
    }

    /// Parses a CLI property argument in `key=value` format.
    pub fn parse_property_arg(arg: &str) -> Result<(String, String), String> {
        let Some((key, val)) = arg.split_once('=') else {
            return Err("formato de propriedade inválido. Use `chave=valor`.".to_string());
        };
        let key = key.trim();
        if key.is_empty() {
            return Err("a chave da propriedade não pode ser vazia.".to_string());
        }
        Ok((key.to_string(), val.trim().to_string()))
    }
}

/// Errors occurring during note selector resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteSelectorError {
    /// Selector is invalid (e.g., contains path characters or is too short).
    InvalidFormat(String),
    /// No note matched the provided selector.
    NotFound(String),
    /// Multiple notes matched the provided prefix.
    Ambiguous(String, Vec<Uuid>),
    /// Store is unavailable or unreadable.
    StoreUnavailable(String),
    /// The note file is a symlink and was refused for security.
    SymlinkRefused(String),
}

impl std::fmt::Display for NoteSelectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat(sel) => {
                write!(
                    f,
                    "identificador inválido `{sel}`. Use um UUID completo ou prefixo hexadecimal com no mínimo 8 caracteres."
                )
            }
            Self::NotFound(sel) => write!(f, "nenhuma nota corresponde a `{sel}`."),
            Self::Ambiguous(sel, _matches) => write!(f, "o identificador `{sel}` é ambíguo."),
            Self::StoreUnavailable(err) => write!(f, "store indisponível: {err}"),
            Self::SymlinkRefused(path) => {
                write!(
                    f,
                    "leitura recusada: o arquivo `{path}` é um link simbólico."
                )
            }
        }
    }
}

impl std::error::Error for NoteSelectorError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{NoteMetadata, NoteProperty};

    #[test]
    fn filter_tag_matching_and_semantics() {
        let meta = NoteMetadata::try_new(
            ["Medicina".into(), "PBL".into(), "Urgência".into()],
            [NoteProperty {
                key: "disciplina".into(),
                value: "cardiologia".into(),
            }],
        )
        .expect("valid metadata");

        // Single tag match (case and accent folded)
        let f1 = NoteFilter::new(vec!["medicina".into()], vec![]);
        assert!(f1.matches(&meta));

        let f2 = NoteFilter::new(vec!["urgencia".into()], vec![]);
        assert!(f2.matches(&meta));

        // Multiple tags AND match
        let f3 = NoteFilter::new(vec!["Medicina".into(), "pbl".into()], vec![]);
        assert!(f3.matches(&meta));

        // One missing tag fails
        let f4 = NoteFilter::new(vec!["Medicina".into(), "Cirurgia".into()], vec![]);
        assert!(!f4.matches(&meta));
    }

    #[test]
    fn filter_property_matching_and_semantics() {
        let meta = NoteMetadata::try_new(
            ["Medicina".into()],
            [
                NoteProperty {
                    key: "disciplina".into(),
                    value: "cardiologia".into(),
                },
                NoteProperty {
                    key: "Status".into(),
                    value: "Revisando".into(),
                },
            ],
        )
        .expect("valid metadata");

        let f1 = NoteFilter::new(vec![], vec![("disciplina".into(), "cardiologia".into())]);
        assert!(f1.matches(&meta));

        // Case and accent folded key and value
        let f2 = NoteFilter::new(vec![], vec![("status".into(), "revisando".into())]);
        assert!(f2.matches(&meta));

        // Combined tag and property match
        let f3 = NoteFilter::new(
            vec!["medicina".into()],
            vec![
                ("disciplina".into(), "cardiologia".into()),
                ("status".into(), "revisando".into()),
            ],
        );
        assert!(f3.matches(&meta));

        // Wrong property value fails
        let f4 = NoteFilter::new(vec![], vec![("disciplina".into(), "neurologia".into())]);
        assert!(!f4.matches(&meta));
    }

    #[test]
    fn parse_property_arg_splits_on_first_equals() {
        let (k, v) = NoteFilter::parse_property_arg("status=revisando").expect("valid");
        assert_eq!(k, "status");
        assert_eq!(v, "revisando");

        let (k, v) = NoteFilter::parse_property_arg("formula=a=b+c").expect("valid with equals");
        assert_eq!(k, "formula");
        assert_eq!(v, "a=b+c");

        assert!(NoteFilter::parse_property_arg("sem_igual").is_err());
        assert!(NoteFilter::parse_property_arg("=sem_chave").is_err());
    }
}
