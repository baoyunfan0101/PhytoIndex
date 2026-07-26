use serde::{Deserialize, Serialize};

use super::hooks::{CompiledHook, load_script};
use super::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

const HOOK_KEY: &str = "synonym_authority_hook";
const HOOK_FUNCTION: &str = "split_synonym_authority";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScientificNameParts {
    pub name: String,
    pub authority_year: Option<String>,
}

pub fn split_scientific_name_authority(value: &str) -> ScientificNameParts {
    default_split_scientific_name_authority(value)
}

pub fn default_split_scientific_name_authority(value: &str) -> ScientificNameParts {
    let value = value.trim();
    if value.is_empty() {
        return ScientificNameParts {
            name: String::new(),
            authority_year: None,
        };
    }

    let mut words = Vec::new();
    let mut word_start = None;
    for (index, character) in value.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = word_start.take() {
                words.push((start, index, &value[start..index]));
            }
        } else if word_start.is_none() {
            word_start = Some(index);
        }
    }
    if let Some(start) = word_start {
        words.push((start, value.len(), &value[start..]));
    }
    let mut uppercase_words = 0;
    let split_at = words.iter().position(|(_, _, word)| {
        word.contains('(')
            || matches!(*word, "de" | "von" | "van")
            || word.chars().next().is_some_and(|character| {
                if character.is_uppercase() {
                    uppercase_words += 1;
                    uppercase_words >= 2
                } else {
                    false
                }
            })
    });

    let (name, authority_year) = match split_at {
        Some(index) if index > 0 => (
            &value[..words[index].0],
            Some(value[words[index].0..].to_string()),
        ),
        _ => (value, None),
    };
    ScientificNameParts {
        name: normalize_taxonomy_name(name).unwrap_or_default(),
        authority_year,
    }
}

pub fn split_scientific_name_authority_with_database(
    database: &Database,
    value: &str,
) -> CoreResult<ScientificNameParts> {
    SynonymAuthorityParser::load(&database.connect()?)?.split(value)
}

pub(crate) struct SynonymAuthorityParser {
    hook: Option<CompiledHook>,
}

impl SynonymAuthorityParser {
    pub(crate) fn load(connection: &rusqlite::Connection) -> CoreResult<Self> {
        match load_script(connection, HOOK_KEY)? {
            Some(script) => Self::from_script(&script),
            None => Ok(Self { hook: None }),
        }
    }

    pub(crate) fn from_script(script: &str) -> CoreResult<Self> {
        Ok(Self {
            hook: Some(CompiledHook::new(script)?),
        })
    }

    pub(crate) fn split(&self, value: &str) -> CoreResult<ScientificNameParts> {
        let output = match self.hook.as_ref() {
            Some(hook) => hook.call(HOOK_FUNCTION, value)?,
            None => default_split_scientific_name_authority(value),
        };
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
                default_split_scientific_name_authority(value),
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
                #{ name: value, authority_year: "custom" }
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            parser.split("  Canis   lupus  ").unwrap(),
            ScientificNameParts {
                name: "Canis lupus".into(),
                authority_year: Some("custom".into()),
            }
        );
    }
}
