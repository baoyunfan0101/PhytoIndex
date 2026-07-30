//! Map settings and cursor-based geotagged photo queries.

use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::metadata::{self, MetadataKey};
use crate::models::{Photo, PhotoPage};
use crate::photos::{
    PhotoCursor, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor, photo_page_limit,
};
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MapTileProvider {
    Osm,
    Tianditu,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MapSettings {
    pub provider: MapTileProvider,
    pub tianditu_token: Option<String>,
}

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            provider: MapTileProvider::Osm,
            tianditu_token: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct MapBounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapPhoto {
    pub photo: Photo,
    pub longitude: f64,
    pub latitude: f64,
}

pub fn get_map_settings(database: &Database) -> CoreResult<MapSettings> {
    Ok(
        metadata::get_json(&database.connect_metadata()?, MetadataKey::MapSettings)?
            .unwrap_or_default(),
    )
}

pub fn set_map_settings(database: &Database, mut settings: MapSettings) -> CoreResult<MapSettings> {
    settings.tianditu_token = settings
        .tianditu_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    metadata::set_json(
        &database.connect_metadata()?,
        MetadataKey::MapSettings,
        &settings,
    )?;
    Ok(settings)
}

pub fn list_map_photos(
    database: &Database,
    bounds: Option<MapBounds>,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<MapPhoto>> {
    if let Some(bounds) = bounds {
        validate_bounds(bounds)?;
    }
    let bounds_scope = bounds.map(bounds_scope);
    let after_photo_id = match decode_photo_cursor(cursor)? {
        None => 0,
        Some(PhotoCursor::MapPhotos { bounds, photo_id }) if bounds == bounds_scope => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let mut parameters = vec![SqlValue::Integer(after_photo_id)];
    let bounds_filter = match bounds {
        None => String::new(),
        Some(bounds) if bounds.west <= bounds.east => {
            parameters.extend([
                SqlValue::Real(bounds.south),
                SqlValue::Real(bounds.north),
                SqlValue::Real(bounds.west),
                SqlValue::Real(bounds.east),
            ]);
            r#"
              AND photo_metadata.latitude BETWEEN ?2 AND ?3
              AND photo_metadata.longitude BETWEEN ?4 AND ?5
            "#
            .to_string()
        }
        Some(bounds) => {
            parameters.extend([
                SqlValue::Real(bounds.south),
                SqlValue::Real(bounds.north),
                SqlValue::Real(bounds.west),
                SqlValue::Real(bounds.east),
            ]);
            r#"
              AND photo_metadata.latitude BETWEEN ?2 AND ?3
              AND (
                  photo_metadata.longitude >= ?4
                  OR photo_metadata.longitude <= ?5
              )
            "#
            .to_string()
        }
    };
    parameters.push(SqlValue::Integer(limit as i64 + 1));
    let limit_parameter = parameters.len();
    let sql = crate::photos::photo_select_with(
        ", photo_metadata.longitude, photo_metadata.latitude",
        &format!(
            r#"
            JOIN photo_metadata ON photo_metadata.photo_id = photos.photo_id
            WHERE photo_metadata.longitude IS NOT NULL
              AND photo_metadata.latitude IS NOT NULL
              AND photos.photo_id > ?1
              {bounds_filter}
            ORDER BY photos.photo_id
            LIMIT ?{limit_parameter}
            "#
        ),
    );
    let connection = database.connect()?;
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(parameters), |row| {
        Ok(MapPhoto {
            photo: crate::db::photo_from_row(row)?,
            longitude: row.get("longitude")?,
            latitude: row.get("latitude")?,
        })
    })?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|item| {
                encode_photo_cursor(&PhotoCursor::MapPhotos {
                    bounds: bounds_scope,
                    photo_id: item.photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

fn validate_bounds(bounds: MapBounds) -> CoreResult<()> {
    if ![bounds.west, bounds.south, bounds.east, bounds.north]
        .into_iter()
        .all(f64::is_finite)
        || !(-180.0..=180.0).contains(&bounds.west)
        || !(-180.0..=180.0).contains(&bounds.east)
        || !(-90.0..=90.0).contains(&bounds.south)
        || !(-90.0..=90.0).contains(&bounds.north)
        || bounds.south > bounds.north
    {
        return Err(CoreError::InvalidArgument("invalid map bounds".into()));
    }
    Ok(())
}

fn bounds_scope(bounds: MapBounds) -> [u64; 4] {
    [
        bounds.west.to_bits(),
        bounds.south.to_bits(),
        bounds.east.to_bits(),
        bounds.north.to_bits(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> (tempfile::TempDir, Database) {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open_test(directory.path().join("test.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                UPDATE photo_library SET root_path = '/photos' WHERE library_id = 1;
                INSERT INTO photo_directories (
                    directory_id, parent_directory_id, name, relative_path
                ) VALUES (1, NULL, '', '');
                INSERT INTO photos (
                    photo_id, directory_id, filename, file_size, modified_at_ns
                ) VALUES
                    (1, 1, 'east.jpg', 1, 1),
                    (2, 1, 'west.jpg', 1, 1),
                    (3, 1, 'missing.jpg', 1, 1);
                INSERT INTO photo_metadata (photo_id, longitude, latitude) VALUES
                    (1, 120.0, 30.0),
                    (2, -120.0, 40.0),
                    (3, NULL, NULL);
                "#,
            )
            .unwrap();
        (directory, database)
    }

    #[test]
    fn stores_normalized_map_settings_through_metadata() {
        let (_directory, database) = database();
        assert_eq!(get_map_settings(&database).unwrap(), MapSettings::default());
        let saved = set_map_settings(
            &database,
            MapSettings {
                provider: MapTileProvider::Tianditu,
                tianditu_token: Some(" token ".to_string()),
            },
        )
        .unwrap();
        assert_eq!(saved.tianditu_token.as_deref(), Some("token"));
        assert_eq!(get_map_settings(&database).unwrap(), saved);
    }

    #[test]
    fn map_pages_are_bounded_and_cursor_scoped() {
        let (_directory, database) = database();
        let first = list_map_photos(&database, None, None, 1).unwrap();
        assert_eq!(first.items[0].photo.photo_id, 1);
        let second = list_map_photos(&database, None, first.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(second.items[0].photo.photo_id, 2);
        assert!(second.next_cursor.is_none());

        let east = MapBounds {
            west: 100.0,
            south: 20.0,
            east: 130.0,
            north: 35.0,
        };
        let page = list_map_photos(&database, Some(east), None, 50).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].photo.photo_id, 1);
        assert!(list_map_photos(&database, Some(east), first.next_cursor.as_deref(), 50).is_err());

        let antimeridian = MapBounds {
            west: 100.0,
            south: 20.0,
            east: -100.0,
            north: 45.0,
        };
        assert_eq!(
            list_map_photos(&database, Some(antimeridian), None, 50)
                .unwrap()
                .items
                .len(),
            2
        );
    }
}
