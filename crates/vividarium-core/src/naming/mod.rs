//! Shared taxonomy-name normalization and configurable Rhai naming hooks.
//!
//! Use this module for filename parsing, synonym-authority splitting, hook
//! settings, bundled templates, and project hook tests.

mod hooks;
mod normalize;
mod photo_filename;
mod synonym;
mod templates;
mod testing;

use serde::{Deserialize, Serialize};

use crate::metadata::{self, MetadataKey};
use crate::{CoreResult, Database};

pub use normalize::normalize_taxonomy_name;
pub use photo_filename::{
    ParsedPhotoFilename, TaxonomicNameInfo, default_parse_photo_filename, parse_photo_filename,
};
pub use synonym::{
    ScientificNameParts, default_split_scientific_name_authority, split_scientific_name_authority,
    split_scientific_name_authority_with_database,
};
pub use templates::{NamingHookTemplates, get_naming_hook_template, get_naming_hook_templates};
pub use testing::{
    NamingHookCaseResult, NamingHookTestCase, NamingHookTestCases, NamingHookTestReport,
    NamingHookTestResult, get_naming_hook_test_cases, run_naming_hook_tests,
    set_naming_hook_test_cases, test_naming_hook,
};

#[cfg(test)]
pub(crate) use hooks::take_compile_count as take_hook_compile_count;
pub(crate) use photo_filename::PhotoFilenameParser;
pub(crate) use synonym::SynonymAuthorityParser;
pub(crate) use testing::seed_default_test_cases;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NamingHookKind {
    PhotoFilename,
    SynonymAuthority,
}

impl NamingHookKind {
    pub(crate) const fn metadata_key(self) -> MetadataKey {
        match self {
            Self::PhotoFilename => MetadataKey::PhotoFilenameHook,
            Self::SynonymAuthority => MetadataKey::SynonymAuthorityHook,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamingHookSettings {
    pub photo_filename: Option<String>,
    pub synonym_authority: Option<String>,
}

pub fn get_naming_hook_settings(database: &Database) -> CoreResult<NamingHookSettings> {
    let connection = database.connect_metadata()?;
    Ok(NamingHookSettings {
        photo_filename: hooks::load_script(&connection, MetadataKey::PhotoFilenameHook)?,
        synonym_authority: hooks::load_script(&connection, MetadataKey::SynonymAuthorityHook)?,
    })
}

pub fn set_naming_hook(
    database: &Database,
    kind: NamingHookKind,
    script: Option<&str>,
) -> CoreResult<()> {
    let script = script.map(str::trim).filter(|value| !value.is_empty());
    if let Some(script) = script {
        test_naming_hook(kind, script, hook_sample(kind))?;
    }
    let mut connection = database.connect_metadata()?;
    let transaction = connection.transaction()?;
    if let Some(script) = script {
        metadata::set_raw(&transaction, kind.metadata_key(), script)?;
    } else {
        metadata::remove(&transaction, kind.metadata_key())?;
    }
    transaction.commit()?;
    if kind == NamingHookKind::PhotoFilename {
        for library in database.list_photo_libraries()? {
            let Ok(connection) = database.connect_photo_library_registration(&library) else {
                continue;
            };
            connection.execute(
                r#"
            INSERT INTO photo_mapping_queue (photo_id, reason)
            SELECT photo_id, 'hook' FROM photos
            WHERE true
            ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason
            "#,
                [],
            )?;
        }
    }
    Ok(())
}

fn hook_sample(kind: NamingHookKind) -> &'static str {
    match kind {
        NamingHookKind::PhotoFilename => "Canis lupus001.jpg",
        NamingHookKind::SynonymAuthority => "Canis lycaon Smith, 1900",
    }
}
