use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use exif::{In, Reader as ExifReader, Tag, Value};
use rusqlite::{Connection, OptionalExtension, params};

use super::{PHOTO_WRITE_LOCK, library_root, load_directory, safe_directory_path, safe_file_path};
use crate::db::{Database, photo_from_row};
use crate::error::{CoreError, CoreResult};
use crate::models::{Photo, PhotoLibraryRegistration, PhotoMetadata};

pub fn photo_file_path(database: &Database, photo_id: i64) -> CoreResult<PathBuf> {
    let library = active_library(database)?;
    photo_file_path_for_library(database, &library.library_uuid, photo_id)
}

pub fn photo_file_path_for_library(
    database: &Database,
    library_uuid: &str,
    photo_id: i64,
) -> CoreResult<PathBuf> {
    let library = database.photo_library(library_uuid)?;
    let connection = database.connect_photo_library_registration(&library)?;
    photo_file_path_from_connection(&connection, photo_id)
}

fn photo_file_path_from_connection(connection: &Connection, photo_id: i64) -> CoreResult<PathBuf> {
    let photo = get_photo_from_connection(connection, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    let root = library_root(connection)?;
    let directory = load_directory(connection, photo.directory_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo directory {}", photo.directory_id)))?;
    let directory = safe_directory_path(&root, &directory.relative_path)?;
    safe_file_path(&root, &directory.join(photo.filename))
}

pub fn photo_directory_path(database: &Database, directory_id: i64) -> CoreResult<PathBuf> {
    let library = active_library(database)?;
    let connection = database.connect_photo_library_registration(&library)?;
    let root = library_root(&connection)?;
    let directory = load_directory(&connection, directory_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo directory {directory_id}")))?;
    safe_directory_path(&root, &directory.relative_path)
}

pub fn get_photo_metadata(database: &Database, photo_id: i64) -> CoreResult<PhotoMetadata> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let library = active_library(database)?;
    let connection = database.connect_photo_library_registration(&library)?;
    if let Some(metadata) = connection
        .query_row(
            "SELECT * FROM photo_metadata WHERE photo_id = ?",
            [photo_id],
            metadata_from_row,
        )
        .optional()?
    {
        return Ok(metadata);
    }
    let path = photo_file_path_from_connection(&connection, photo_id)?;
    let metadata = read_file_metadata(photo_id, &path);
    connection.execute(
        r#"
        INSERT INTO photo_metadata (
            photo_id, captured_at, camera, width, height, longitude, latitude, exif_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(photo_id) DO UPDATE SET
            captured_at = excluded.captured_at,
            camera = excluded.camera,
            width = excluded.width,
            height = excluded.height,
            longitude = excluded.longitude,
            latitude = excluded.latitude,
            exif_json = excluded.exif_json
        "#,
        params![
            metadata.photo_id,
            metadata.captured_at,
            metadata.camera,
            metadata.width,
            metadata.height,
            metadata.longitude,
            metadata.latitude,
            metadata.exif_json,
        ],
    )?;
    Ok(metadata)
}

pub fn get_or_create_thumbnail(
    database: &Database,
    photo_id: i64,
    thumbnail_root: &Path,
) -> CoreResult<PathBuf> {
    let library = active_library(database)?;
    get_or_create_thumbnail_for_library(database, &library.library_uuid, photo_id, thumbnail_root)
}

pub fn get_or_create_thumbnail_for_library(
    database: &Database,
    library_uuid: &str,
    photo_id: i64,
    thumbnail_root: &Path,
) -> CoreResult<PathBuf> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let library = database.photo_library(library_uuid)?;
    let connection = database.connect_photo_library_registration(&library)?;
    let photo = get_photo_from_connection(&connection, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    let library_thumbnail_root = thumbnail_root.join(library_uuid);
    if let Some(existing) = &photo.thumbnail_path {
        let path = PathBuf::from(existing);
        if path.starts_with(&library_thumbnail_root) && path.is_file() {
            return Ok(path);
        }
    }
    let source = photo_file_path_from_connection(&connection, photo_id)?;
    fs::create_dir_all(&library_thumbnail_root)?;
    let output = library_thumbnail_root.join(format!(
        "photo_{}_{}_{}.webp",
        photo.photo_id, photo.modified_at_ns, photo.file_size
    ));
    image::open(&source)?
        .thumbnail(256, 256)
        .save_with_format(&output, image::ImageFormat::WebP)?;
    connection.execute(
        "UPDATE photos SET thumbnail_path = ? WHERE photo_id = ?",
        params![output.to_string_lossy(), photo_id],
    )?;
    Ok(output)
}

pub fn rebase_thumbnail_paths(database: &Database, thumbnail_root: &Path) -> CoreResult<usize> {
    let library = active_library(database)?;
    let library_thumbnail_root = thumbnail_root.join(&library.library_uuid);
    let mut connection = database.connect_photo_library_registration(&library)?;
    let transaction = connection.transaction()?;
    let paths = {
        let mut statement = transaction.prepare(
            "SELECT photo_id, thumbnail_path FROM photos WHERE thumbnail_path IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut updated = 0;
    for (photo_id, current) in paths {
        let Some(filename) = Path::new(&current).file_name() else {
            continue;
        };
        let candidate = library_thumbnail_root.join(filename);
        if candidate.is_file() && candidate.as_path() != Path::new(&current) {
            transaction.execute(
                "UPDATE photos SET thumbnail_path = ? WHERE photo_id = ?",
                params![candidate.to_string_lossy(), photo_id],
            )?;
            updated += 1;
        } else if !Path::new(&current).starts_with(&library_thumbnail_root) {
            transaction.execute(
                "UPDATE photos SET thumbnail_path = NULL WHERE photo_id = ?",
                [photo_id],
            )?;
            updated += 1;
        }
    }
    transaction.commit()?;
    Ok(updated)
}

fn active_library(database: &Database) -> CoreResult<PhotoLibraryRegistration> {
    database
        .active_photo_library()?
        .ok_or_else(|| CoreError::InvalidArgument("no active photo library is registered".into()))
}

fn get_photo_from_connection(connection: &Connection, photo_id: i64) -> CoreResult<Option<Photo>> {
    connection
        .query_row(
            &super::photo_select("WHERE photos.photo_id = ?"),
            [photo_id],
            photo_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn metadata_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoMetadata> {
    Ok(PhotoMetadata {
        photo_id: row.get("photo_id")?,
        captured_at: row.get("captured_at")?,
        camera: row.get("camera")?,
        width: row.get("width")?,
        height: row.get("height")?,
        longitude: row.get("longitude")?,
        latitude: row.get("latitude")?,
        exif_json: row.get("exif_json")?,
    })
}

fn read_file_metadata(photo_id: i64, path: &Path) -> PhotoMetadata {
    let dimensions = image::ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.into_dimensions().ok());
    let exif = File::open(path).ok().and_then(|file| {
        ExifReader::new()
            .read_from_container(&mut BufReader::new(file))
            .ok()
    });
    let mut result = PhotoMetadata {
        photo_id,
        captured_at: None,
        camera: None,
        width: dimensions.map(|value| value.0 as i64),
        height: dimensions.map(|value| value.1 as i64),
        longitude: None,
        latitude: None,
        exif_json: None,
    };
    let Some(exif) = exif else {
        return result;
    };
    let mut values = BTreeMap::new();
    for field in exif.fields() {
        values.insert(
            format!("{}", field.tag),
            field.display_value().with_unit(&exif).to_string(),
        );
    }
    result.exif_json =
        (!values.is_empty()).then(|| serde_json::to_string(&values).unwrap_or_default());
    result.captured_at = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .into_iter()
        .find_map(|tag| exif.get_field(tag, In::PRIMARY))
        .and_then(|field| parse_exif_datetime(&field.display_value().to_string()));
    let make = exif
        .get_field(Tag::Make, In::PRIMARY)
        .map(|field| clean_exif_text(&field.display_value().to_string()));
    let model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .map(|field| clean_exif_text(&field.display_value().to_string()));
    result.camera = match (make, model) {
        (Some(make), Some(model)) if !model.contains(&make) => Some(format!("{make} {model}")),
        (_, Some(model)) => Some(model),
        (Some(make), None) => Some(make),
        _ => None,
    };
    result.latitude = gps_coordinate(
        exif.get_field(Tag::GPSLatitude, In::PRIMARY),
        exif.get_field(Tag::GPSLatitudeRef, In::PRIMARY),
    );
    result.longitude = gps_coordinate(
        exif.get_field(Tag::GPSLongitude, In::PRIMARY),
        exif.get_field(Tag::GPSLongitudeRef, In::PRIMARY),
    );
    result
}

fn clean_exif_text(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches('\0')
        .trim()
        .to_string()
}

fn parse_exif_datetime(value: &str) -> Option<String> {
    let value = clean_exif_text(value);
    ["%Y:%m:%d %H:%M:%S", "%Y-%m-%d %H:%M:%S"]
        .into_iter()
        .find_map(|format| NaiveDateTime::parse_from_str(&value, format).ok())
        .map(|date| date.format("%Y-%m-%d %H:%M:%S").to_string())
}

fn gps_coordinate(value: Option<&exif::Field>, reference: Option<&exif::Field>) -> Option<f64> {
    let Value::Rational(values) = &value?.value else {
        return None;
    };
    if values.len() < 3 {
        return None;
    }
    let mut coordinate =
        values[0].to_f64() + values[1].to_f64() / 60.0 + values[2].to_f64() / 3600.0;
    let direction = reference
        .map(|field| clean_exif_text(&field.display_value().to_string()))
        .unwrap_or_default();
    if matches!(direction.as_str(), "S" | "W") {
        coordinate = -coordinate;
    }
    Some(coordinate)
}
