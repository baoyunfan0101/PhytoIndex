use serde::{Deserialize, Serialize};

use super::hooks::{CompiledHook, load_script};
use super::normalize_taxonomy_name;
use super::templates::PHOTO_FILENAME_TEMPLATE;
use crate::metadata::MetadataKey;
use crate::{CoreError, CoreResult, Database};

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
    PhotoFilenameParser::load(&database.connect_metadata()?)?.parse(filename)
}

pub fn default_parse_photo_filename(filename: &str) -> CoreResult<ParsedPhotoFilename> {
    PhotoFilenameParser::from_script(PHOTO_FILENAME_TEMPLATE)?.parse(filename)
}

pub(crate) struct PhotoFilenameParser {
    hook: CompiledHook,
}

impl PhotoFilenameParser {
    pub(crate) fn load(connection: &rusqlite::Connection) -> CoreResult<Self> {
        let script = load_script(connection, MetadataKey::PhotoFilenameHook)?;
        Self::from_script(script.as_deref().unwrap_or(PHOTO_FILENAME_TEMPLATE))
    }

    pub(crate) fn from_script(script: &str) -> CoreResult<Self> {
        Ok(Self {
            hook: CompiledHook::new(script)?,
        })
    }

    pub(crate) fn parse(&self, filename: &str) -> CoreResult<ParsedPhotoFilename> {
        let output = self.hook.call(HOOK_FUNCTION, filename)?;
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

    #[test]
    fn default_template_accepts_inputs_longer_than_the_array_limit() {
        let input = format!("Canis {}001.jpg", "a".repeat(80));
        let parsed = default_parse_photo_filename(&input).unwrap();
        assert_eq!(parsed.info.genus_sci.as_deref(), Some("Canis"));
        assert_eq!(parsed.suffix, "001.jpg");
    }
}
