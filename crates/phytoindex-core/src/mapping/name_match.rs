use std::collections::HashSet;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::naming::TaxonomicNameInfo;
use crate::taxonomy::{TaxonRank, TaxonomyNameType};
use crate::{CoreError, CoreResult, Database};

const METADATA_KEY: &str = "photo_name_match_settings";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PhotoNameField {
    SpeciesSci,
    SpeciesZh,
    GenusSci,
    GenusZh,
    FamilySci,
    FamilyZh,
}

impl PhotoNameField {
    pub(crate) const ALL: [Self; 6] = [
        Self::SpeciesSci,
        Self::SpeciesZh,
        Self::GenusSci,
        Self::GenusZh,
        Self::FamilySci,
        Self::FamilyZh,
    ];

    pub(crate) fn value(self, info: &TaxonomicNameInfo) -> Option<&str> {
        match self {
            Self::SpeciesSci => info.species_sci.as_deref(),
            Self::SpeciesZh => info.species_zh.as_deref(),
            Self::GenusSci => info.genus_sci.as_deref(),
            Self::GenusZh => info.genus_zh.as_deref(),
            Self::FamilySci => info.family_sci.as_deref(),
            Self::FamilyZh => info.family_zh.as_deref(),
        }
    }

    pub(crate) fn rank(self) -> TaxonRank {
        match self {
            Self::SpeciesSci | Self::SpeciesZh => TaxonRank::Species,
            Self::GenusSci | Self::GenusZh => TaxonRank::Genus,
            Self::FamilySci | Self::FamilyZh => TaxonRank::Family,
        }
    }

    pub(crate) fn name_types(self) -> [TaxonomyNameType; 2] {
        match self {
            Self::SpeciesSci | Self::GenusSci | Self::FamilySci => {
                [TaxonomyNameType::SciName, TaxonomyNameType::Synonym]
            }
            Self::SpeciesZh | Self::GenusZh | Self::FamilyZh => {
                [TaxonomyNameType::ZhName, TaxonomyNameType::ZhAlias]
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoNameMatchSettings {
    pub priority: Vec<PhotoNameField>,
}

impl Default for PhotoNameMatchSettings {
    fn default() -> Self {
        Self {
            priority: PhotoNameField::ALL.to_vec(),
        }
    }
}

pub fn get_photo_name_match_settings(database: &Database) -> CoreResult<PhotoNameMatchSettings> {
    load(&database.connect()?)
}

pub fn set_photo_name_match_settings(
    database: &Database,
    settings: &PhotoNameMatchSettings,
) -> CoreResult<()> {
    validate(settings)?;
    let value = serde_json::to_string(settings)
        .map_err(|error| CoreError::InvalidArgument(error.to_string()))?;
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    transaction.execute(
        r#"
        INSERT INTO app_metadata (metadata_key, metadata_value)
        VALUES (?, ?)
        ON CONFLICT(metadata_key) DO UPDATE SET metadata_value = excluded.metadata_value
        "#,
        params![METADATA_KEY, value],
    )?;
    transaction.execute(
        r#"
        INSERT INTO photo_mapping_queue (photo_id, reason)
        SELECT photo_id, 'settings' FROM photos
        WHERE true
        ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason
        "#,
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn load(connection: &rusqlite::Connection) -> CoreResult<PhotoNameMatchSettings> {
    use rusqlite::OptionalExtension;

    let value = connection
        .query_row(
            "SELECT metadata_value FROM app_metadata WHERE metadata_key = ?",
            [METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let settings = match value {
        Some(value) => serde_json::from_str(&value).map_err(|error| {
            CoreError::InvalidArgument(format!("invalid match settings: {error}"))
        })?,
        None => PhotoNameMatchSettings::default(),
    };
    validate(&settings)?;
    Ok(settings)
}

fn validate(settings: &PhotoNameMatchSettings) -> CoreResult<()> {
    let values = settings.priority.iter().copied().collect::<HashSet<_>>();
    if settings.priority.len() != PhotoNameField::ALL.len()
        || values.len() != PhotoNameField::ALL.len()
        || !PhotoNameField::ALL
            .into_iter()
            .all(|field| values.contains(&field))
    {
        return Err(CoreError::InvalidArgument(
            "photo name priority must contain each field exactly once".into(),
        ));
    }
    Ok(())
}
