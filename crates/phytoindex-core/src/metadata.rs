use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{CoreError, CoreResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataKey {
    MapSettings,
    PhotoFilenameFormatSettings,
    PhotoFilenameHook,
    PhotoFilenameHookTests,
    PhotoNameMatchSettings,
    SynonymAuthorityHook,
    SynonymAuthorityHookTests,
    TaxonomyNameSeparator,
}

impl MetadataKey {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::MapSettings => "map_settings",
            Self::PhotoFilenameFormatSettings => "photo_filename_format_settings",
            Self::PhotoFilenameHook => "photo_filename_hook",
            Self::PhotoFilenameHookTests => "photo_filename_hook_tests",
            Self::PhotoNameMatchSettings => "photo_name_match_settings",
            Self::SynonymAuthorityHook => "synonym_authority_hook",
            Self::SynonymAuthorityHookTests => "synonym_authority_hook_tests",
            Self::TaxonomyNameSeparator => "taxonomy_name_separator",
        }
    }
}

pub(crate) fn get_raw(connection: &Connection, key: MetadataKey) -> CoreResult<Option<String>> {
    connection
        .query_row(
            "SELECT metadata_value FROM app_metadata WHERE metadata_key = ?",
            [key.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
}

pub(crate) fn set_raw(connection: &Connection, key: MetadataKey, value: &str) -> CoreResult<()> {
    connection.execute(
        r#"
        INSERT INTO app_metadata (metadata_key, metadata_value)
        VALUES (?, ?)
        ON CONFLICT(metadata_key) DO UPDATE
        SET metadata_value = excluded.metadata_value
        "#,
        params![key.as_str(), value],
    )?;
    Ok(())
}

pub(crate) fn insert_raw_if_missing(
    connection: &Connection,
    key: MetadataKey,
    value: &str,
) -> CoreResult<()> {
    connection.execute(
        r#"
        INSERT INTO app_metadata (metadata_key, metadata_value)
        VALUES (?, ?)
        ON CONFLICT(metadata_key) DO NOTHING
        "#,
        params![key.as_str(), value],
    )?;
    Ok(())
}

pub(crate) fn remove(connection: &Connection, key: MetadataKey) -> CoreResult<()> {
    connection.execute(
        "DELETE FROM app_metadata WHERE metadata_key = ?",
        [key.as_str()],
    )?;
    Ok(())
}

pub(crate) fn get_json<T: DeserializeOwned>(
    connection: &Connection,
    key: MetadataKey,
) -> CoreResult<Option<T>> {
    get_raw(connection, key)?
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                CoreError::InvalidArgument(format!("invalid {} metadata: {error}", key.as_str()))
            })
        })
        .transpose()
}

pub(crate) fn set_json<T: Serialize>(
    connection: &Connection,
    key: MetadataKey,
    value: &T,
) -> CoreResult<()> {
    let value = serde_json::to_string(value).map_err(|error| {
        CoreError::InvalidArgument(format!("invalid {} metadata: {error}", key.as_str()))
    })?;
    set_raw(connection, key, &value)
}

pub(crate) fn insert_json_if_missing<T: Serialize>(
    connection: &Connection,
    key: MetadataKey,
    value: &T,
) -> CoreResult<()> {
    let value = serde_json::to_string(value).map_err(|error| {
        CoreError::InvalidArgument(format!("invalid {} metadata: {error}", key.as_str()))
    })?;
    insert_raw_if_missing(connection, key, &value)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Settings {
        enabled: bool,
    }

    #[test]
    fn owns_raw_and_json_metadata_access() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE app_metadata (
                    metadata_key TEXT PRIMARY KEY,
                    metadata_value TEXT NOT NULL
                );
                "#,
            )
            .unwrap();

        set_raw(&connection, MetadataKey::TaxonomyNameSeparator, ";").unwrap();
        assert_eq!(
            get_raw(&connection, MetadataKey::TaxonomyNameSeparator).unwrap(),
            Some(";".to_string())
        );

        let settings = Settings { enabled: true };
        set_json(&connection, MetadataKey::MapSettings, &settings).unwrap();
        assert_eq!(
            get_json::<Settings>(&connection, MetadataKey::MapSettings).unwrap(),
            Some(settings)
        );

        remove(&connection, MetadataKey::MapSettings).unwrap();
        assert_eq!(
            get_raw(&connection, MetadataKey::MapSettings).unwrap(),
            None
        );
    }
}
