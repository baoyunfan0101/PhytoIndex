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
    let filename = normalize_legacy_quotes(filename);
    let (information, suffix) = split_information_suffix(&filename)?;
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

fn normalize_legacy_quotes(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\u{2018}' | '\u{2019}' | '\u{201c}' | '\u{201d}' | '\u{ff07}' => '\'',
            _ => character,
        })
        .collect()
}

fn split_information_suffix(filename: &str) -> CoreResult<(&str, String)> {
    let mut quoted = false;
    let mut cutoff = filename.len();
    for (index, character) in filename.char_indices() {
        if character == '\'' {
            if quoted && apostrophe_is_internal(filename, index) {
                continue;
            }
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
    if quoted {
        return Err(CoreError::InvalidArgument(
            "photo filename contains an unmatched quote".into(),
        ));
    }
    let raw_information = &filename[..cutoff];
    let mut information = raw_information.trim_end();
    let mut prefix = "";
    for marker in ["YN", "M", "F"] {
        if raw_information.ends_with(marker) {
            let start = raw_information.len() - marker.len();
            information = raw_information[..start].trim_end();
            prefix = &filename[start..cutoff];
            break;
        }
    }
    Ok((information, format!("{prefix}{}", &filename[cutoff..])))
}

fn apostrophe_is_internal(value: &str, index: usize) -> bool {
    let remainder = &value[index + '\''.len_utf8()..];
    let Some(next) = remainder.chars().next() else {
        return false;
    };
    next.is_ascii_alphabetic() && next != 'M' && next != 'F' && !remainder.starts_with("YN")
}

fn parse_information(value: &str) -> TaxonomicNameInfo {
    let (chinese, scientific) = split_chinese_scientific(value);
    let mut output = TaxonomicNameInfo::default();
    parse_chinese(chinese, &mut output);
    parse_scientific(scientific, &mut output);
    output
}

fn split_chinese_scientific(value: &str) -> (&str, &str) {
    let Some(first) = value.chars().next() else {
        return ("", "");
    };
    if first.is_ascii() {
        return ("", value);
    }
    let mut end = 0;
    let mut characters = value.char_indices().peekable();
    while let Some((index, character)) = characters.next() {
        match character {
            '\'' => {
                end = consume_group(index, '\'', &mut characters);
            }
            '(' => {
                end = consume_group(index, ')', &mut characters);
            }
            _ if !character.is_ascii() => {
                end = index + character.len_utf8();
            }
            _ => break,
        }
    }
    (&value[..end], value[end..].trim_start())
}

fn consume_group(
    start: usize,
    closing: char,
    characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> usize {
    let mut end = start + 1;
    for (index, character) in characters.by_ref() {
        end = index + character.len_utf8();
        if character == closing {
            break;
        }
    }
    end
}

fn parse_chinese(mut value: &str, output: &mut TaxonomicNameInfo) {
    if let Some(index) = value.find('\u{79d1}') {
        let end = index + '\u{79d1}'.len_utf8();
        if !value[end..].starts_with('\u{79d1}') {
            output.family_zh = normalize_taxonomy_name(&value[..end]);
            value = value[end..].trim();
        }
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
    match value.rfind(' ') {
        None => {
            if (value.len() > 6 && value.ends_with("aceae"))
                || (value.len() > 5 && value.ends_with("idae"))
            {
                output.family_sci = Some(value);
            } else {
                output.genus_sci = Some(value);
            }
        }
        Some(separator) if value[..separator].trim_end() == "x" => {
            output.genus_sci = Some(value);
        }
        Some(separator) => {
            output.genus_sci = normalize_taxonomy_name(&value[..separator]);
            output.species_sci = Some(value);
        }
    }
}

#[cfg(test)]
#[path = "photo_filename_golden.rs"]
mod golden_tests;

#[cfg(test)]
mod tests {
    use super::*;

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
