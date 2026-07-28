use serde::{Deserialize, Serialize};

use super::hooks::{CompiledHook, load_script};
use super::normalize_taxonomy_name;
use super::templates::SYNONYM_AUTHORITY_TEMPLATE;
use crate::metadata::MetadataKey;
use crate::{CoreError, CoreResult, Database};

const HOOK_FUNCTION: &str = "split_synonym_authority";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScientificNameParts {
    pub name: String,
    pub authority_year: Option<String>,
}

pub fn split_scientific_name_authority(value: &str) -> CoreResult<ScientificNameParts> {
    default_split_scientific_name_authority(value)
}

pub fn default_split_scientific_name_authority(value: &str) -> CoreResult<ScientificNameParts> {
    SynonymAuthorityParser::from_script(SYNONYM_AUTHORITY_TEMPLATE)?.split(value)
}

pub fn split_scientific_name_authority_with_database(
    database: &Database,
    value: &str,
) -> CoreResult<ScientificNameParts> {
    SynonymAuthorityParser::load(&database.connect_metadata()?)?.split(value)
}

pub(crate) struct SynonymAuthorityParser {
    hook: CompiledHook,
}

impl SynonymAuthorityParser {
    pub(crate) fn load(connection: &rusqlite::Connection) -> CoreResult<Self> {
        let script = load_script(connection, MetadataKey::SynonymAuthorityHook)?;
        Self::from_script(script.as_deref().unwrap_or(SYNONYM_AUTHORITY_TEMPLATE))
    }

    pub(crate) fn from_script(script: &str) -> CoreResult<Self> {
        Ok(Self {
            hook: CompiledHook::new(script)?,
        })
    }

    pub(crate) fn split(&self, value: &str) -> CoreResult<ScientificNameParts> {
        let output = self.hook.call(HOOK_FUNCTION, value)?;
        normalize_output(output)
    }
}

fn normalize_output(mut output: ScientificNameParts) -> CoreResult<ScientificNameParts> {
    output.name = normalize_taxonomy_name(&output.name)
        .ok_or_else(|| CoreError::InvalidArgument("synonym hook returned an empty name".into()))?;
    output.authority_year = output
        .authority_year
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Ok(output)
}

#[cfg(test)]
#[path = "synonym_golden.rs"]
mod golden_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_supported_authority_forms() {
        for (value, name, authority) in [
            (
                "Canis lupus (Linnaeus, 1758)",
                "Canis lupus",
                "(Linnaeus, 1758)",
            ),
            (
                "Canis lupus Linnaeus, 1758",
                "Canis lupus",
                "Linnaeus, 1758",
            ),
            (
                "Canis lupus de Silva, 1900",
                "Canis lupus",
                "de Silva, 1900",
            ),
        ] {
            assert_eq!(
                default_split_scientific_name_authority(value).unwrap(),
                ScientificNameParts {
                    name: name.into(),
                    authority_year: Some(authority.into()),
                }
            );
        }
    }

    #[test]
    fn runs_a_custom_hook() {
        let parser = SynonymAuthorityParser::from_script(
            r#"
            fn split_synonym_authority(value) {
                let authority = if value == "  Canis   lupus  " {
                    "raw"
                } else {
                    "changed"
                };
                #{ name: value, authority_year: authority }
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            parser.split("  Canis   lupus  ").unwrap(),
            ScientificNameParts {
                name: "Canis lupus".into(),
                authority_year: Some("raw".into()),
            }
        );
    }
}
