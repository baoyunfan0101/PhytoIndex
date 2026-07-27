use serde::{Deserialize, Serialize};

use super::NamingHookKind;

pub(crate) const PHOTO_FILENAME_TEMPLATE: &str = include_str!("templates/photo_filename.rhai");
pub(crate) const SYNONYM_AUTHORITY_TEMPLATE: &str =
    include_str!("templates/synonym_authority.rhai");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookTemplates {
    pub photo_filename: String,
    pub synonym_authority: String,
}

pub fn get_naming_hook_templates() -> NamingHookTemplates {
    NamingHookTemplates {
        photo_filename: PHOTO_FILENAME_TEMPLATE.to_string(),
        synonym_authority: SYNONYM_AUTHORITY_TEMPLATE.to_string(),
    }
}

pub fn get_naming_hook_template(kind: NamingHookKind) -> &'static str {
    match kind {
        NamingHookKind::PhotoFilename => PHOTO_FILENAME_TEMPLATE,
        NamingHookKind::SynonymAuthority => SYNONYM_AUTHORITY_TEMPLATE,
    }
}
