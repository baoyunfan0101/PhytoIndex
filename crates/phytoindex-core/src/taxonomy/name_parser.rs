use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScientificNameParts {
    pub name: String,
    pub authority_year: Option<String>,
}

pub fn split_scientific_name_authority(value: &str) -> ScientificNameParts {
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

    match split_at {
        Some(index) if index > 0 => ScientificNameParts {
            name: value[..words[index].0].trim_end().to_string(),
            authority_year: Some(value[words[index].0..].to_string()),
        },
        _ => ScientificNameParts {
            name: value.to_string(),
            authority_year: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_supported_authority_forms() {
        assert_eq!(
            split_scientific_name_authority("Canis lupus (Linnaeus, 1758)"),
            ScientificNameParts {
                name: "Canis lupus".into(),
                authority_year: Some("(Linnaeus, 1758)".into()),
            }
        );
        assert_eq!(
            split_scientific_name_authority("Canis lupus Linnaeus, 1758")
                .authority_year
                .as_deref(),
            Some("Linnaeus, 1758")
        );
        assert_eq!(
            split_scientific_name_authority("Canis lupus von Meyer, 1848")
                .authority_year
                .as_deref(),
            Some("von Meyer, 1848")
        );
        assert_eq!(
            split_scientific_name_authority("Canis  lupus  (Linnaeus, 1758)"),
            ScientificNameParts {
                name: "Canis  lupus".into(),
                authority_year: Some("(Linnaeus, 1758)".into()),
            }
        );
    }
}
