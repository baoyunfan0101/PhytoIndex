//! General application settings and restorable workspace state.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::metadata::{self, MetadataKey};
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GeneralSettings {
    pub theme: ThemePreference,
    pub restore_tabs: bool,
    pub recent_searches_limit: u8,
    #[serde(default = "default_taxon_tree_name_parts")]
    pub taxon_tree_name_parts: TaxonTreeNameParts,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TaxonTreeNameParts {
    pub sci_name: bool,
    pub zh_name: bool,
    pub en_name: bool,
}

impl Default for TaxonTreeNameParts {
    fn default() -> Self {
        Self {
            sci_name: true,
            zh_name: true,
            en_name: true,
        }
    }
}

fn default_taxon_tree_name_parts() -> TaxonTreeNameParts {
    TaxonTreeNameParts::default()
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::Dark,
            restore_tabs: true,
            recent_searches_limit: 10,
            taxon_tree_name_parts: TaxonTreeNameParts::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceTabKind {
    Folders,
    PhotoTaxonomy,
    Map,
    PhotoHistory,
    Mapping,
    TaxonomySearch,
    FormattedUpdate,
    CustomSql,
    TaxonomyHistory,
    Settings,
    SearchPhotos,
    TaxonPhotos,
    PhotoDetail,
    MappingEditor,
    TaxonDetail,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum WorkspaceSettingsSection {
    General,
    Storage,
    #[serde(rename = "Photo Libraries")]
    PhotoLibraries,
    #[serde(rename = "Taxonomy Databases")]
    TaxonomyDatabases,
    Naming,
    Map,
    #[serde(rename = "Filename Parser")]
    FilenameParser,
    #[serde(rename = "Synonym Splitter")]
    SynonymSplitter,
    About,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceTab {
    pub id: String,
    pub kind: WorkspaceTabKind,
    pub title: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub taxon_id: Option<i64>,
    #[serde(default)]
    pub photo_id: Option<i64>,
    #[serde(default)]
    pub settings_section: Option<WorkspaceSettingsSection>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkspaceState {
    pub opened_tabs: Vec<WorkspaceTab>,
    pub active_tab: Option<String>,
}

pub fn get_general_settings(database: &Database) -> CoreResult<GeneralSettings> {
    Ok(
        metadata::get_json(&database.connect_metadata()?, MetadataKey::GeneralSettings)?
            .unwrap_or_default(),
    )
}

pub fn update_general_settings(
    database: &Database,
    settings: &GeneralSettings,
) -> CoreResult<GeneralSettings> {
    validate_general_settings(settings)?;
    metadata::set_json(
        &database.connect_metadata()?,
        MetadataKey::GeneralSettings,
        settings,
    )?;
    Ok(settings.clone())
}

pub fn get_workspace_state(database: &Database) -> CoreResult<WorkspaceState> {
    let connection = database.connect_metadata()?;
    let Some(raw) = metadata::get_raw(&connection, MetadataKey::WorkspaceState)? else {
        return Ok(WorkspaceState::default());
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn save_workspace_state(
    database: &Database,
    workspace_state: &WorkspaceState,
) -> CoreResult<()> {
    validate_workspace_state(workspace_state)?;
    metadata::set_json(
        &database.connect_metadata()?,
        MetadataKey::WorkspaceState,
        workspace_state,
    )
}

fn validate_general_settings(settings: &GeneralSettings) -> CoreResult<()> {
    if !(1..=50).contains(&settings.recent_searches_limit) {
        return Err(CoreError::InvalidArgument(
            "recent_searches_limit must be between 1 and 50".to_string(),
        ));
    }
    if !settings.taxon_tree_name_parts.sci_name
        && !settings.taxon_tree_name_parts.zh_name
        && !settings.taxon_tree_name_parts.en_name
    {
        return Err(CoreError::InvalidArgument(
            "at least one taxon tree name part must be visible".to_string(),
        ));
    }
    Ok(())
}

fn validate_workspace_state(workspace_state: &WorkspaceState) -> CoreResult<()> {
    let mut ids = HashSet::new();
    for tab in &workspace_state.opened_tabs {
        if tab.id.trim().is_empty() || tab.title.trim().is_empty() {
            return Err(CoreError::InvalidArgument(
                "workspace tab id and title must not be empty".to_string(),
            ));
        }
        if !ids.insert(tab.id.as_str()) {
            return Err(CoreError::InvalidArgument(format!(
                "workspace contains duplicate tab id: {}",
                tab.id
            )));
        }
        match tab.kind {
            WorkspaceTabKind::SearchPhotos if tab.query.as_deref().is_none_or(str::is_empty) => {
                return Err(CoreError::InvalidArgument(
                    "search-photos workspace tabs require a query".to_string(),
                ));
            }
            WorkspaceTabKind::TaxonPhotos | WorkspaceTabKind::TaxonDetail
                if tab.taxon_id.is_none() =>
            {
                return Err(CoreError::InvalidArgument(
                    "taxon workspace tabs require taxon_id".to_string(),
                ));
            }
            WorkspaceTabKind::PhotoDetail | WorkspaceTabKind::MappingEditor
                if tab.photo_id.is_none() =>
            {
                return Err(CoreError::InvalidArgument(
                    "photo workspace tabs require photo_id".to_string(),
                ));
            }
            _ => {}
        }
    }
    if let Some(active_tab) = workspace_state.active_tab.as_deref()
        && !ids.contains(active_tab)
    {
        return Err(CoreError::InvalidArgument(
            "active_tab must refer to an opened tab".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn test_database() -> (tempfile::TempDir, Database) {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        (directory, database)
    }

    #[test]
    fn returns_general_defaults_when_metadata_is_missing() {
        let (_directory, database) = test_database();
        assert_eq!(
            get_general_settings(&database).unwrap(),
            GeneralSettings {
                theme: ThemePreference::Dark,
                restore_tabs: true,
                recent_searches_limit: 10,
                taxon_tree_name_parts: TaxonTreeNameParts::default(),
            }
        );
    }

    #[test]
    fn stores_and_reads_general_settings() {
        let (_directory, database) = test_database();
        let settings = GeneralSettings {
            theme: ThemePreference::Light,
            restore_tabs: false,
            recent_searches_limit: 24,
            taxon_tree_name_parts: TaxonTreeNameParts {
                sci_name: true,
                zh_name: false,
                en_name: true,
            },
        };
        assert_eq!(
            update_general_settings(&database, &settings).unwrap(),
            settings
        );
        assert_eq!(get_general_settings(&database).unwrap(), settings);
    }

    #[test]
    fn rejects_recent_search_limits_outside_the_supported_range() {
        let (_directory, database) = test_database();
        for limit in [0, 51] {
            let settings = GeneralSettings {
                recent_searches_limit: limit,
                ..GeneralSettings::default()
            };
            assert!(matches!(
                update_general_settings(&database, &settings),
                Err(CoreError::InvalidArgument(_))
            ));
        }
    }

    #[test]
    fn rejects_hidden_taxon_tree_name_parts() {
        let (_directory, database) = test_database();
        let settings = GeneralSettings {
            taxon_tree_name_parts: TaxonTreeNameParts {
                sci_name: false,
                zh_name: false,
                en_name: false,
            },
            ..GeneralSettings::default()
        };
        assert!(matches!(
            update_general_settings(&database, &settings),
            Err(CoreError::InvalidArgument(_))
        ));
    }

    #[test]
    fn stores_workspace_tabs_and_active_tab() {
        let (_directory, database) = test_database();
        let workspace = WorkspaceState {
            opened_tabs: vec![WorkspaceTab {
                id: "settings".to_string(),
                kind: WorkspaceTabKind::Settings,
                title: "Settings".to_string(),
                query: None,
                taxon_id: None,
                photo_id: None,
                settings_section: Some(WorkspaceSettingsSection::General),
            }],
            active_tab: Some("settings".to_string()),
        };
        save_workspace_state(&database, &workspace).unwrap();
        assert_eq!(get_workspace_state(&database).unwrap(), workspace);
    }

    #[test]
    fn treats_unreadable_workspace_metadata_as_empty() {
        let (_directory, database) = test_database();
        metadata::set_raw(
            &database.connect_metadata().unwrap(),
            MetadataKey::WorkspaceState,
            "not-json",
        )
        .unwrap();
        assert_eq!(
            get_workspace_state(&database).unwrap(),
            WorkspaceState::default()
        );
    }

    #[test]
    fn rejects_invalid_workspace_references() {
        let (_directory, database) = test_database();
        let workspace = WorkspaceState {
            opened_tabs: vec![WorkspaceTab {
                id: "search".to_string(),
                kind: WorkspaceTabKind::SearchPhotos,
                title: "Search".to_string(),
                query: None,
                taxon_id: None,
                photo_id: None,
                settings_section: None,
            }],
            active_tab: Some("missing".to_string()),
        };
        assert!(matches!(
            save_workspace_state(&database, &workspace),
            Err(CoreError::InvalidArgument(_))
        ));
    }
}
