use serde::{Deserialize, Serialize};

use super::hooks::{CompiledHook, load_script};
use super::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

const HOOK_KEY: &str = "photo_filename_hook";
const HOOK_FUNCTION: &str = "parse_photo_filename";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TaxonomicNameInfo {
    pub family_sci: Option<String>,
    pub genus_sci: Option<String>,
    pub species_sci: Option<String>,
    pub family_zh: Option<String>,
    pub genus_zh: Option<String>,
    pub species_zh: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ParsedPhotoFilename {
    pub info: TaxonomicNameInfo,
    pub suffix: String,
}

pub fn parse_photo_filename(
    database: &Database,
    filename: &str,
) -> CoreResult<ParsedPhotoFilename> {
    PhotoFilenameParser::load(&database.connect()?)?.parse(filename)
}

pub fn default_parse_photo_filename(filename: &str) -> CoreResult<ParsedPhotoFilename> {
    let filename = std::path::Path::new(filename)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::InvalidArgument("photo filename is not valid UTF-8".into()))?;
    let (information, suffix) = split_information_suffix(filename);
    let Some(information) = normalize_taxonomy_name(information) else {
        return Ok(ParsedPhotoFilename {
            info: TaxonomicNameInfo::default(),
            suffix,
        });
    };
    Ok(ParsedPhotoFilename {
        info: parse_information(&information),
        suffix,
    })
}

pub(crate) struct PhotoFilenameParser {
    hook: Option<CompiledHook>,
}

impl PhotoFilenameParser {
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

    pub(crate) fn parse(&self, filename: &str) -> CoreResult<ParsedPhotoFilename> {
        let output = match self.hook.as_ref() {
            Some(hook) => hook.call(HOOK_FUNCTION, filename)?,
            None => default_parse_photo_filename(filename)?,
        };
        normalize_output(output)
    }
}

fn normalize_output(mut output: ParsedPhotoFilename) -> CoreResult<ParsedPhotoFilename> {
    for value in [
        &mut output.info.family_sci,
        &mut output.info.genus_sci,
        &mut output.info.species_sci,
        &mut output.info.family_zh,
        &mut output.info.genus_zh,
        &mut output.info.species_zh,
    ] {
        *value = value.take().and_then(|name| normalize_taxonomy_name(&name));
    }
    if output.suffix.contains('/') || output.suffix.contains('\\') {
        return Err(CoreError::InvalidArgument(
            "photo filename hook returned a suffix containing a path".into(),
        ));
    }
    Ok(output)
}

fn split_information_suffix(filename: &str) -> (&str, String) {
    let mut quoted = false;
    let mut cutoff = filename.len();
    for (index, character) in filename.char_indices() {
        if matches!(character, '\'' | '\u{2018}' | '\u{2019}' | '\u{ff07}') {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        if character.is_ascii_digit() {
            cutoff = index;
            break;
        }
        if character == '.' {
            let next = filename[index + 1..].chars().next();
            if next != Some(' ') {
                cutoff = index;
                break;
            }
        }
    }
    let mut information = filename[..cutoff].trim_end();
    let mut prefix = "";
    for marker in ["YN", "M", "F"] {
        if information.ends_with(marker) {
            let start = information.len() - marker.len();
            information = information[..start].trim_end();
            prefix = &filename[start..cutoff];
            break;
        }
    }
    (information, format!("{prefix}{}", &filename[cutoff..]))
}

fn parse_information(value: &str) -> TaxonomicNameInfo {
    let scientific_start = value
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphabetic())
        .map(|(index, _)| index);
    let (chinese, scientific) = match scientific_start {
        Some(index) => (value[..index].trim(), value[index..].trim()),
        None => (value.trim(), ""),
    };
    let mut output = TaxonomicNameInfo::default();
    parse_chinese(chinese, &mut output);
    parse_scientific(scientific, &mut output);
    if output.family_zh.is_none() && output.genus_zh.is_none() && output.species_zh.is_none() {
        return output;
    }
    if scientific.is_empty() {
        return output;
    }
    output
}

fn parse_chinese(mut value: &str, output: &mut TaxonomicNameInfo) {
    if let Some(index) = value.find('\u{79d1}') {
        let end = index + '\u{79d1}'.len_utf8();
        output.family_zh = normalize_taxonomy_name(&value[..end]);
        value = value[end..].trim();
    }
    if let Some(index) = value.find('\u{5c5e}') {
        let end = index + '\u{5c5e}'.len_utf8();
        output.genus_zh = normalize_taxonomy_name(&value[..end]);
        value = value[end..].trim();
    }
    if !value.is_empty() {
        output.species_zh = normalize_taxonomy_name(value);
    }
}

fn parse_scientific(value: &str, output: &mut TaxonomicNameInfo) {
    let Some(value) = normalize_taxonomy_name(value) else {
        return;
    };
    let words = value.split_whitespace().collect::<Vec<_>>();
    if words.len() == 1 {
        if value.ends_with("aceae") || value.ends_with("idae") {
            output.family_sci = Some(value);
        } else {
            output.genus_sci = Some(value);
        }
    } else {
        output.genus_sci = Some(words[0].to_string());
        output.species_sci = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scientific_names_and_preserves_suffix() {
        assert_eq!(
            default_parse_photo_filename("Canis lupus020.jpg").unwrap(),
            ParsedPhotoFilename {
                info: TaxonomicNameInfo {
                    genus_sci: Some("Canis".into()),
                    species_sci: Some("Canis lupus".into()),
                    ..TaxonomicNameInfo::default()
                },
                suffix: "020.jpg".into(),
            }
        );
        assert_eq!(
            default_parse_photo_filename("Canidae.jpg")
                .unwrap()
                .info
                .family_sci,
            Some("Canidae".into())
        );
    }

    #[test]
    fn runs_a_custom_hook() {
        let parser = PhotoFilenameParser::from_script(
            r#"
            fn parse_photo_filename(filename) {
                #{
                    info: #{ family_sci: filename },
                    suffix: ".jpg"
                }
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            parser.parse("  Canidae  ").unwrap(),
            ParsedPhotoFilename {
                info: TaxonomicNameInfo {
                    family_sci: Some("Canidae".into()),
                    ..TaxonomicNameInfo::default()
                },
                suffix: ".jpg".into(),
            }
        );
    }
}
