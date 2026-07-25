use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::db::{Database, photo_from_row};
use crate::error::{CoreError, CoreResult};
use crate::mapping;
use crate::models::{
    DirectoryEntryCounts, NewPhoto, Photo, PhotoDirectory, PhotoLibrary, PhotoSyncResult,
};

pub use crate::models::{PhotoDirectoryItem, PhotoPage};

mod media;
mod operations;
mod page;

pub use media::{
    get_or_create_thumbnail, get_photo_metadata, photo_file_path, rebase_thumbnail_paths,
};
pub use operations::{
    PhotoOperation, PhotoOperationInput, PhotoOperationItem, PhotoOperationSource,
    export_photo_operation_inputs, get_photo_operation, list_photo_operations,
    revert_photo_operation,
};
use operations::{insert_photo_operation, insert_photo_operation_item};
pub(crate) use page::{
    PhotoCursor, PhotoPageSection, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor,
    photo_page_limit,
};

const IMAGE_EXTENSIONS: &[&str] = &[
    "arw", "bmp", "cr2", "cr3", "dng", "gif", "heic", "jpeg", "jpg", "nef", "png", "raf", "rw2",
    "tif", "tiff", "webp",
];
static PHOTO_WRITE_LOCK: Mutex<()> = Mutex::new(());

pub type ProgressCallback<'a> = dyn FnMut(u64, Option<u64>, &str) + Send + 'a;

#[derive(Debug)]
struct ScannedDirectory {
    name: String,
}

#[derive(Debug)]
struct ScannedPhoto {
    filename: String,
    file_size: i64,
    modified_at_ns: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoRenameRowStatus {
    Applied,
    NoChange,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoRenameRowOutcome {
    pub row_number: usize,
    pub photo_id: i64,
    pub operation_id: Option<i64>,
    pub status: PhotoRenameRowStatus,
    pub message: String,
    pub photo: Option<Photo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoRenameOperationResult {
    pub operation_id: Option<i64>,
    pub rows: Vec<PhotoRenameRowOutcome>,
}

struct PhotoRenameResult {
    photo: Photo,
    operation_id: Option<i64>,
}

pub fn open_library(database: &Database, root: &str) -> CoreResult<PhotoLibrary> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let root = normalize_root(root)?;
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let current = transaction
        .query_row(
            "SELECT root_path FROM photo_library WHERE library_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current.as_deref() != Some(root.as_str()) {
        transaction.execute("DELETE FROM photo_taxon_usage", [])?;
        transaction.execute("DELETE FROM photo_directories", [])?;
        transaction.execute("DELETE FROM photo_library", [])?;
        transaction.execute(
            "INSERT INTO photo_library (library_id, root_path) VALUES (1, ?)",
            [&root],
        )?;
        transaction.execute(
            "INSERT INTO photo_directories (parent_directory_id, name, relative_path) VALUES (NULL, '', '')",
            [],
        )?;
    }
    transaction.commit()?;
    get_library(database)?.ok_or_else(|| CoreError::NotFound("photo library".into()))
}

pub fn get_library(database: &Database) -> CoreResult<Option<PhotoLibrary>> {
    let connection = database.connect()?;
    connection
        .query_row(
            r#"
            SELECT photo_library.root_path, root.directory_id
            FROM photo_library
            JOIN photo_directories AS root ON root.relative_path = ''
            WHERE photo_library.library_id = 1
            "#,
            [],
            |row| {
                Ok(PhotoLibrary {
                    root_path: row.get(0)?,
                    root_directory_id: row.get(1)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
}

pub fn get_photo_count(database: &Database) -> CoreResult<i64> {
    let connection = database.connect()?;
    Ok(connection.query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))?)
}

pub fn get_directory_counts(
    database: &Database,
    directory_id: i64,
) -> CoreResult<DirectoryEntryCounts> {
    let connection = database.connect()?;
    if load_directory(&connection, directory_id)?.is_none() {
        return Err(CoreError::NotFound(format!(
            "photo directory {directory_id}"
        )));
    }
    Ok(DirectoryEntryCounts {
        directory_count: connection.query_row(
            "SELECT COUNT(*) FROM photo_directories WHERE parent_directory_id = ?",
            [directory_id],
            |row| row.get(0),
        )?,
        file_count: connection.query_row(
            "SELECT COUNT(*) FROM photos WHERE directory_id = ?",
            [directory_id],
            |row| row.get(0),
        )?,
    })
}

pub fn get_photo(database: &Database, photo_id: i64) -> CoreResult<Option<Photo>> {
    let connection = database.connect()?;
    connection
        .query_row(
            &photo_select("WHERE photos.photo_id = ?"),
            [photo_id],
            photo_from_row,
        )
        .optional()
        .map_err(Into::into)
}

pub fn list_photos(database: &Database) -> CoreResult<Vec<Photo>> {
    let connection = database.connect()?;
    let mut statement = connection.prepare(&photo_select("ORDER BY photos.photo_id"))?;
    let rows = statement.query_map([], photo_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn browse_directory(
    database: &Database,
    directory_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoDirectoryItem>> {
    let connection = database.connect()?;
    load_directory(&connection, directory_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo directory {directory_id}")))?;
    let (section, after_name, after_id) = match decode_photo_cursor(cursor)? {
        None => (PhotoPageSection::Containers, String::new(), 0),
        Some(PhotoCursor::DirectoryEntries {
            directory_id: cursor_directory_id,
            section,
            name,
            item_id,
        }) if cursor_directory_id == directory_id => (section, name, item_id),
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let mut directories = Vec::new();
    let mut remaining = limit;
    let mut has_more = false;

    if section == PhotoPageSection::Containers {
        let mut statement = connection.prepare(
            r#"
            SELECT directory_id, parent_directory_id, name, relative_path
            FROM photo_directories
            WHERE parent_directory_id = ?1
              AND (?2 = '' OR name > ?2 OR (name = ?2 AND directory_id > ?3))
            ORDER BY name, directory_id
            LIMIT ?4
            "#,
        )?;
        let rows = statement.query_map(
            params![
                directory_id,
                after_name.as_str(),
                after_id,
                remaining as i64 + 1
            ],
            directory_from_row,
        )?;
        directories = rows.collect::<Result<Vec<_>, _>>()?;
        if directories.len() > remaining {
            directories.pop();
            let next_cursor = directories
                .last()
                .map(|value| {
                    encode_photo_cursor(&PhotoCursor::DirectoryEntries {
                        directory_id,
                        section: PhotoPageSection::Containers,
                        name: value.name.clone(),
                        item_id: value.directory_id,
                    })
                })
                .transpose()?;
            return Ok(PhotoPage {
                items: directories
                    .into_iter()
                    .map(|directory| PhotoDirectoryItem::Directory { directory })
                    .collect(),
                next_cursor,
            });
        }
        remaining -= directories.len();
    }

    let (after_name, after_id) = if section == PhotoPageSection::Photos {
        (after_name.as_str(), after_id)
    } else {
        ("", 0)
    };
    let mut statement = connection.prepare(&photo_select(
        r#"
        WHERE photos.directory_id = ?1
          AND (?2 = '' OR photos.filename > ?2
               OR (photos.filename = ?2 AND photos.photo_id > ?3))
        ORDER BY photos.filename, photos.photo_id
        LIMIT ?4
        "#,
    ))?;
    let rows = statement.query_map(
        params![directory_id, after_name, after_id, remaining as i64 + 1],
        photo_from_row,
    )?;
    let mut files = rows.collect::<Result<Vec<_>, _>>()?;
    if files.len() > remaining {
        files.pop();
        has_more = true;
    }

    let next_cursor = if has_more {
        if let Some(value) = files.last() {
            Some(encode_photo_cursor(&PhotoCursor::DirectoryEntries {
                directory_id,
                section: PhotoPageSection::Photos,
                name: value.filename.clone(),
                item_id: value.photo_id,
            })?)
        } else {
            directories
                .last()
                .map(|value| {
                    encode_photo_cursor(&PhotoCursor::DirectoryEntries {
                        directory_id,
                        section: PhotoPageSection::Containers,
                        name: value.name.clone(),
                        item_id: value.directory_id,
                    })
                })
                .transpose()?
        }
    } else {
        None
    };
    let mut items = directories
        .into_iter()
        .map(|directory| PhotoDirectoryItem::Directory { directory })
        .collect::<Vec<_>>();
    items.extend(
        files
            .into_iter()
            .map(|photo| PhotoDirectoryItem::Photo { photo }),
    );
    Ok(PhotoPage { items, next_cursor })
}

pub fn refresh_directory(database: &Database, directory_id: i64) -> CoreResult<PhotoSyncResult> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    refresh_directory_locked(database, directory_id)
}

fn refresh_directory_locked(database: &Database, directory_id: i64) -> CoreResult<PhotoSyncResult> {
    let connection = database.connect()?;
    let directory = load_directory(&connection, directory_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo directory {directory_id}")))?;
    let root = library_root(&connection)?;
    let path = safe_directory_path(&root, &directory.relative_path)?;
    let (scanned_directories, scanned_photos) = scan_directory(&path)?;
    drop(connection);

    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let existing_directories = direct_directories(&transaction, directory_id)?;
    let existing_photos = direct_photos(&transaction, directory_id)?;
    let scanned_directory_names = scanned_directories
        .iter()
        .map(|value| value.name.as_str())
        .collect::<HashSet<_>>();
    let scanned_photo_names = scanned_photos
        .iter()
        .map(|value| value.filename.as_str())
        .collect::<HashSet<_>>();
    let removed_directory_ids = existing_directories
        .iter()
        .filter_map(|(name, value)| {
            (!scanned_directory_names.contains(name.as_str())).then_some(value.directory_id)
        })
        .collect::<Vec<_>>();
    let removed_photo_ids = existing_photos
        .iter()
        .filter_map(|(name, value)| {
            (!scanned_photo_names.contains(name.as_str())).then_some(value.photo_id)
        })
        .collect::<Vec<_>>();

    mapping::remove_directory_mappings(&transaction, &removed_directory_ids)?;
    mapping::remove_photo_mappings(&transaction, &removed_photo_ids)?;
    for id in &removed_directory_ids {
        transaction.execute("DELETE FROM photo_directories WHERE directory_id = ?", [id])?;
    }
    for id in &removed_photo_ids {
        transaction.execute("DELETE FROM photos WHERE photo_id = ?", [id])?;
    }

    let mut directories_inserted = 0;
    for entry in scanned_directories {
        if existing_directories.contains_key(&entry.name) {
            continue;
        }
        let relative_path = join_relative_path(&directory.relative_path, &entry.name);
        transaction.execute(
            "INSERT INTO photo_directories (parent_directory_id, name, relative_path) VALUES (?, ?, ?)",
            params![directory_id, entry.name, relative_path],
        )?;
        directories_inserted += 1;
    }

    let mut inserted = 0;
    let mut updated = 0;
    let mut unchanged = 0;
    let mut changed_photo_ids = Vec::new();
    for entry in scanned_photos {
        match existing_photos.get(&entry.filename) {
            None => {
                let photo_id = insert_photo(
                    &transaction,
                    &NewPhoto {
                        directory_id,
                        filename: entry.filename,
                        file_size: entry.file_size,
                        modified_at_ns: entry.modified_at_ns,
                        thumbnail_path: None,
                    },
                )?;
                changed_photo_ids.push(photo_id);
                inserted += 1;
            }
            Some(photo)
                if photo.file_size == entry.file_size
                    && photo.modified_at_ns == entry.modified_at_ns =>
            {
                unchanged += 1;
            }
            Some(photo) => {
                transaction.execute(
                    r#"
                    UPDATE photos
                    SET file_size = ?, modified_at_ns = ?, thumbnail_path = NULL
                    WHERE photo_id = ?
                    "#,
                    params![entry.file_size, entry.modified_at_ns, photo.photo_id],
                )?;
                transaction.execute(
                    "DELETE FROM photo_metadata WHERE photo_id = ?",
                    [photo.photo_id],
                )?;
                changed_photo_ids.push(photo.photo_id);
                updated += 1;
            }
        }
    }
    mapping::queue_photo_ids(&transaction, &changed_photo_ids, "refresh")?;
    transaction.commit()?;
    Ok(PhotoSyncResult {
        directory_id,
        inserted,
        unchanged,
        updated,
        deleted: removed_photo_ids.len(),
        directories_inserted,
        directories_deleted: removed_directory_ids.len(),
    })
}

pub fn rename_photo(database: &Database, photo_id: i64, new_filename: &str) -> CoreResult<Photo> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let mut operation_id = None;
    let input = [PhotoOperationInput {
        row_number: 1,
        photo_id,
        requested_filename: Some(new_filename.to_string()),
    }];
    Ok(rename_photo_locked(
        database,
        photo_id,
        new_filename,
        PhotoOperationSource::ManualRename,
        1,
        &mut operation_id,
        &input,
    )?
    .photo)
}

pub fn rename_photo_from_taxon(database: &Database, photo_id: i64) -> CoreResult<Photo> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let new_filename = taxon_filename(database, photo_id)?;
    let mut operation_id = None;
    let input = [PhotoOperationInput {
        row_number: 1,
        photo_id,
        requested_filename: None,
    }];
    Ok(rename_photo_locked(
        database,
        photo_id,
        &new_filename,
        PhotoOperationSource::TaxonRename,
        1,
        &mut operation_id,
        &input,
    )?
    .photo)
}

pub fn rename_photos_from_taxa(
    database: &Database,
    photo_ids: &[i64],
) -> CoreResult<PhotoRenameOperationResult> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let input = photo_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, photo_id)| PhotoOperationInput {
            row_number: index + 1,
            photo_id,
            requested_filename: None,
        })
        .collect::<Vec<_>>();
    let mut operation_id = None;
    let mut rows = Vec::with_capacity(photo_ids.len());
    for (index, photo_id) in photo_ids.iter().copied().enumerate() {
        let row_number = index + 1;
        let result = taxon_filename(database, photo_id).and_then(|new_filename| {
            rename_photo_locked(
                database,
                photo_id,
                &new_filename,
                PhotoOperationSource::TaxonSelectionRename,
                row_number,
                &mut operation_id,
                &input,
            )
        });
        rows.push(match result {
            Ok(result) => {
                let status = if result.operation_id.is_some() {
                    PhotoRenameRowStatus::Applied
                } else {
                    PhotoRenameRowStatus::NoChange
                };
                PhotoRenameRowOutcome {
                    row_number,
                    photo_id,
                    operation_id: result.operation_id,
                    status,
                    message: match status {
                        PhotoRenameRowStatus::Applied => "applied".into(),
                        PhotoRenameRowStatus::NoChange => "no change".into(),
                        PhotoRenameRowStatus::Failed => unreachable!(),
                    },
                    photo: Some(result.photo),
                }
            }
            Err(error) if is_photo_rename_row_error(&error) => PhotoRenameRowOutcome {
                row_number,
                photo_id,
                operation_id: None,
                status: PhotoRenameRowStatus::Failed,
                message: error.to_string(),
                photo: None,
            },
            Err(error) => return Err(error),
        });
    }
    Ok(PhotoRenameOperationResult { operation_id, rows })
}

pub fn rename_photos_in_directory_from_taxa(
    database: &Database,
    directory_id: i64,
    include_descendants: bool,
) -> CoreResult<PhotoRenameOperationResult> {
    let connection = database.connect()?;
    if load_directory(&connection, directory_id)?.is_none() {
        return Err(CoreError::NotFound(format!(
            "photo directory {directory_id}"
        )));
    }
    let sql = if include_descendants {
        r#"
        WITH RECURSIVE directories(directory_id) AS (
            SELECT ?1
            UNION ALL
            SELECT photo_directories.directory_id
            FROM photo_directories
            JOIN directories
              ON photo_directories.parent_directory_id = directories.directory_id
        )
        SELECT photos.photo_id
        FROM photos
        JOIN directories USING (directory_id)
        JOIN photo_taxon_mapping USING (photo_id)
        WHERE photo_taxon_mapping.status = 'matched'
          AND NOT EXISTS (
              SELECT 1 FROM photo_mapping_queue
              WHERE photo_mapping_queue.photo_id = photos.photo_id
          )
        ORDER BY photos.directory_id, photos.filename, photos.photo_id
        "#
    } else {
        r#"
        SELECT photos.photo_id
        FROM photos
        JOIN photo_taxon_mapping USING (photo_id)
        WHERE photos.directory_id = ?1
          AND photo_taxon_mapping.status = 'matched'
          AND NOT EXISTS (
              SELECT 1 FROM photo_mapping_queue
              WHERE photo_mapping_queue.photo_id = photos.photo_id
          )
        ORDER BY photos.filename, photos.photo_id
        "#
    };
    let photo_ids = connection
        .prepare(sql)?
        .query_map([directory_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(connection);
    rename_photos_from_taxa(database, &photo_ids)
}

fn is_photo_rename_row_error(error: &CoreError) -> bool {
    matches!(
        error,
        CoreError::Io(_)
            | CoreError::InvalidArgument(_)
            | CoreError::UnsafePath(_)
            | CoreError::NotFound(_)
    )
}

fn rename_photo_locked(
    database: &Database,
    photo_id: i64,
    new_filename: &str,
    operation_source: PhotoOperationSource,
    row_number: usize,
    operation_id: &mut Option<i64>,
    operation_input: &[PhotoOperationInput],
) -> CoreResult<PhotoRenameResult> {
    let new_filename = validate_filename(new_filename)?;
    let old_photo = get_photo(database, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    if old_photo.filename == new_filename {
        return Ok(PhotoRenameResult {
            photo: old_photo,
            operation_id: None,
        });
    }
    let connection = database.connect()?;
    let root = library_root(&connection)?;
    let directory = load_directory(&connection, old_photo.directory_id)?.ok_or_else(|| {
        CoreError::NotFound(format!("photo directory {}", old_photo.directory_id))
    })?;
    let root_path = root
        .to_str()
        .ok_or_else(|| CoreError::InvalidArgument("photo root is not valid UTF-8".into()))?;
    let directory_path = safe_directory_path(&root, &directory.relative_path)?;
    let source = directory_path.join(&old_photo.filename);
    let destination = directory_path.join(&new_filename);
    let temporary = directory_path.join(format!(".vividarium-rename-{photo_id}.tmp"));
    rename_file(&source, &destination, &temporary)?;

    let existing_operation_id = *operation_id;
    let result = (|| -> CoreResult<(Photo, i64)> {
        let mut connection = database.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE photos SET filename = ? WHERE photo_id = ?",
            params![new_filename, photo_id],
        )?;
        mapping::remap_photo_ids(&transaction, &[photo_id])?;
        transaction.execute(
            "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
            [photo_id],
        )?;
        let current_operation_id = match existing_operation_id {
            Some(operation_id) => operation_id,
            None => {
                insert_photo_operation(&transaction, operation_source, root_path, operation_input)?
            }
        };
        insert_photo_operation_item(
            &transaction,
            current_operation_id,
            row_number,
            photo_id,
            &directory.relative_path,
            &old_photo.filename,
            &new_filename,
        )?;
        let photo = transaction
            .query_row(
                &photo_select("WHERE photos.photo_id = ?"),
                [photo_id],
                photo_from_row,
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
        transaction.commit()?;
        Ok((photo, current_operation_id))
    })();
    match result {
        Ok((photo, current_operation_id)) => {
            *operation_id = Some(current_operation_id);
            Ok(PhotoRenameResult {
                photo,
                operation_id: Some(current_operation_id),
            })
        }
        Err(error) => match rename_file(&destination, &source, &temporary) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "photo database update failed: {error}; filesystem rollback failed: {rollback_error}"
            ))),
        },
    }
}

fn taxon_filename(database: &Database, photo_id: i64) -> CoreResult<String> {
    let photo = get_photo(database, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    let connection = database.connect()?;
    let scientific_name = connection
        .query_row(
            r#"
            SELECT taxon_names.name
            FROM photo_taxon_mapping
            JOIN taxon_names USING (taxon_id)
            WHERE photo_taxon_mapping.photo_id = ?1
              AND photo_taxon_mapping.status = 'matched'
              AND taxon_names.name_type = 'sci_name'
              AND NOT EXISTS (
                  SELECT 1
                  FROM photo_mapping_queue
                  WHERE photo_mapping_queue.photo_id = ?1
              )
            "#,
            [photo_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::InvalidArgument(format!(
                "photo {photo_id} must have a matched taxon with an accepted scientific name"
            ))
        })?;
    let extension = Path::new(&photo.filename)
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::InvalidArgument("photo filename has no extension".into()))?;
    Ok(format!("{scientific_name}.{extension}"))
}

fn rename_file(source: &Path, destination: &Path, temporary: &Path) -> CoreResult<()> {
    let destination_is_source = destination.exists()
        && matches!(
            (source.canonicalize(), destination.canonicalize()),
            (Ok(source), Ok(destination)) if source == destination
        );
    if destination.exists() && !destination_is_source {
        return Err(CoreError::InvalidArgument(format!(
            "rename destination already exists: {}",
            destination.display()
        )));
    }
    let case_only_rename = destination_is_source
        || source.parent() == destination.parent()
            && source
                .file_name()
                .and_then(|value| value.to_str())
                .zip(destination.file_name().and_then(|value| value.to_str()))
                .is_some_and(|(left, right)| left != right && left.eq_ignore_ascii_case(right));
    if !case_only_rename {
        fs::rename(source, destination)?;
        return Ok(());
    }
    if temporary.exists() {
        return Err(CoreError::InvalidArgument(format!(
            "temporary rename path already exists: {}",
            temporary.display()
        )));
    }
    fs::rename(source, temporary)?;
    if let Err(error) = fs::rename(temporary, destination) {
        return match fs::rename(temporary, source) {
            Ok(()) => Err(error.into()),
            Err(restore_error) => Err(CoreError::Consistency(format!(
                "rename failed: {error}; source restoration failed: {restore_error}"
            ))),
        };
    }
    Ok(())
}

fn photo_select(suffix: &str) -> String {
    format!(
        r#"
        SELECT photos.photo_id, photos.directory_id,
               CASE WHEN photo_directories.relative_path = '' THEN photos.filename
                    ELSE photo_directories.relative_path || '/' || photos.filename END AS relative_path,
               photos.filename, photos.file_size, photos.modified_at_ns, photos.thumbnail_path
        FROM photos
        JOIN photo_directories ON photo_directories.directory_id = photos.directory_id
        {suffix}
        "#
    )
}

fn load_directory(
    connection: &rusqlite::Connection,
    directory_id: i64,
) -> CoreResult<Option<PhotoDirectory>> {
    connection
        .query_row(
            r#"
            SELECT directory_id, parent_directory_id, name, relative_path
            FROM photo_directories WHERE directory_id = ?
            "#,
            [directory_id],
            directory_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn directory_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoDirectory> {
    Ok(PhotoDirectory {
        directory_id: row.get(0)?,
        parent_directory_id: row.get(1)?,
        name: row.get(2)?,
        relative_path: row.get(3)?,
    })
}

fn direct_directories(
    transaction: &Transaction<'_>,
    directory_id: i64,
) -> CoreResult<HashMap<String, PhotoDirectory>> {
    let mut statement = transaction.prepare(
        r#"
        SELECT directory_id, parent_directory_id, name, relative_path
        FROM photo_directories WHERE parent_directory_id = ?
        "#,
    )?;
    let rows = statement.query_map([directory_id], directory_from_row)?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|value| (value.name.clone(), value))
        .collect())
}

fn direct_photos(
    transaction: &Transaction<'_>,
    directory_id: i64,
) -> CoreResult<HashMap<String, Photo>> {
    let mut statement = transaction.prepare(&photo_select("WHERE photos.directory_id = ?"))?;
    let rows = statement.query_map([directory_id], photo_from_row)?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|value| (value.filename.clone(), value))
        .collect())
}

fn insert_photo(transaction: &Transaction<'_>, photo: &NewPhoto) -> CoreResult<i64> {
    transaction.execute(
        r#"
        INSERT INTO photos (directory_id, filename, file_size, modified_at_ns, thumbnail_path)
        VALUES (?, ?, ?, ?, ?)
        "#,
        params![
            photo.directory_id,
            photo.filename,
            photo.file_size,
            photo.modified_at_ns,
            photo.thumbnail_path,
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn scan_directory(path: &Path) -> CoreResult<(Vec<ScannedDirectory>, Vec<ScannedPhoto>)> {
    let mut directories = Vec::new();
    let mut photos = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| CoreError::InvalidArgument("photo path is not valid UTF-8".into()))?;
        if file_type.is_dir() {
            directories.push(ScannedDirectory { name });
        } else if file_type.is_file() && is_image_filename(&name) {
            let metadata = entry.metadata()?;
            photos.push(ScannedPhoto {
                filename: name,
                file_size: i64::try_from(metadata.len()).map_err(|_| {
                    CoreError::InvalidArgument("photo file size exceeds i64".into())
                })?,
                modified_at_ns: modified_at_ns(&metadata)?,
            });
        }
    }
    Ok((directories, photos))
}

fn normalize_root(root: &str) -> CoreResult<String> {
    let path = PathBuf::from(root);
    let canonical = path.canonicalize()?;
    if !canonical.is_dir() {
        return Err(CoreError::InvalidArgument(format!(
            "photo root is not a directory: {}",
            canonical.display()
        )));
    }
    canonical
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| CoreError::InvalidArgument("photo root is not valid UTF-8".into()))
}

fn library_root(connection: &rusqlite::Connection) -> CoreResult<PathBuf> {
    let value = connection
        .query_row(
            "SELECT root_path FROM photo_library WHERE library_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("photo library".into()))?;
    Ok(PathBuf::from(value).canonicalize()?)
}

fn safe_directory_path(root: &Path, relative_path: &str) -> CoreResult<PathBuf> {
    let relative = Path::new(relative_path);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(CoreError::UnsafePath(relative.into()));
    }
    safe_file_path(root, &root.join(relative))
}

fn safe_file_path(root: &Path, candidate: &Path) -> CoreResult<PathBuf> {
    let candidate = candidate.canonicalize()?;
    if !candidate.starts_with(root) {
        return Err(CoreError::UnsafePath(candidate));
    }
    Ok(candidate)
}

fn validate_filename(value: &str) -> CoreResult<String> {
    let value = value.trim();
    let path = Path::new(value);
    if value.is_empty()
        || path.components().count() != 1
        || matches!(value, "." | "..")
        || value.contains(['/', '\\'])
        || !is_image_filename(value)
    {
        return Err(CoreError::InvalidArgument(
            "photo filename must be one image filename without a path".into(),
        ));
    }
    Ok(value.into())
}

fn is_image_filename(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn join_relative_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.into()
    } else {
        format!("{parent}/{name}")
    }
}

fn modified_at_ns(metadata: &fs::Metadata) -> CoreResult<i64> {
    let value = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CoreError::InvalidArgument(error.to_string()))?
        .as_nanos();
    i64::try_from(value)
        .map_err(|_| CoreError::InvalidArgument("photo modified time exceeds i64".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_refreshes_only_the_requested_directory() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("nested")).unwrap();
        fs::write(root.path().join("first.jpg"), b"first").unwrap();
        fs::write(root.path().join("nested").join("second.jpg"), b"second").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        let result = refresh_directory(&database, library.root_directory_id).unwrap();
        assert_eq!(result.inserted, 1);
        assert_eq!(result.directories_inserted, 1);
        assert_eq!(list_photos(&database).unwrap().len(), 1);
        let listing = browse_directory(&database, library.root_directory_id, None, 20).unwrap();
        assert_eq!(listing.items.len(), 2);
        let nested_directory_id = match &listing.items[0] {
            PhotoDirectoryItem::Directory { directory } => directory.directory_id,
            PhotoDirectoryItem::Photo { .. } => panic!("expected a directory first"),
        };
        assert!(matches!(listing.items[1], PhotoDirectoryItem::Photo { .. }));
        assert_eq!(
            get_directory_counts(&database, library.root_directory_id).unwrap(),
            DirectoryEntryCounts {
                directory_count: 1,
                file_count: 1,
            }
        );
        refresh_directory(&database, nested_directory_id).unwrap();
        assert_eq!(list_photos(&database).unwrap().len(), 2);
        assert_eq!(get_photo_count(&database).unwrap(), 2);
    }

    #[test]
    fn directory_cursor_is_absent_on_the_last_page() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("a")).unwrap();
        fs::create_dir(root.path().join("b")).unwrap();
        fs::write(root.path().join("photo.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();

        let first = browse_directory(&database, library.root_directory_id, None, 2).unwrap();
        assert_eq!(first.items.len(), 2);
        assert!(
            first
                .items
                .iter()
                .all(|item| matches!(item, PhotoDirectoryItem::Directory { .. }))
        );
        let first_directory_id = match first.items[0] {
            PhotoDirectoryItem::Directory { ref directory } => directory.directory_id,
            PhotoDirectoryItem::Photo { .. } => unreachable!(),
        };
        let error = browse_directory(
            &database,
            first_directory_id,
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid photo cursor"));
        let second = browse_directory(
            &database,
            library.root_directory_id,
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap();
        assert_eq!(second.items.len(), 1);
        assert!(matches!(second.items[0], PhotoDirectoryItem::Photo { .. }));
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn renames_the_real_file_and_updates_the_database() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("before.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);
        let renamed = rename_photo(&database, photo.photo_id, "after.jpg").unwrap();
        assert_eq!(renamed.filename, "after.jpg");
        assert_eq!(
            mapping::get_photo_mapping(&database, photo.photo_id)
                .unwrap()
                .unwrap()
                .status,
            mapping::PhotoTaxonStatus::Unmatched
        );
        assert!(!root.path().join("before.jpg").exists());
        assert!(root.path().join("after.jpg").is_file());
    }

    #[test]
    fn records_and_reverts_photo_rename_operations() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("before.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);

        rename_photo(&database, photo.photo_id, "after.jpg").unwrap();

        let operations = list_photo_operations(&database, None, 10).unwrap().items;
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].source, PhotoOperationSource::ManualRename);
        assert_eq!(
            operations[0].root_path,
            root.path().canonicalize().unwrap().to_str().unwrap()
        );
        assert_eq!(operations[0].items.len(), 1);
        assert_eq!(operations[0].items[0].row_number, 1);
        assert_eq!(operations[0].items[0].photo_id, photo.photo_id);
        assert_eq!(operations[0].items[0].directory_relative_path, "");
        assert_eq!(operations[0].items[0].old_filename, "before.jpg");
        assert_eq!(operations[0].items[0].new_filename, "after.jpg");

        revert_photo_operation(&database, operations[0].operation_id).unwrap();

        assert!(root.path().join("before.jpg").is_file());
        assert!(!root.path().join("after.jpg").exists());
        assert_eq!(
            get_photo(&database, photo.photo_id)
                .unwrap()
                .unwrap()
                .filename,
            "before.jpg"
        );
        assert!(
            list_photo_operations(&database, None, 10)
                .unwrap()
                .items
                .is_empty()
        );
        let error = revert_photo_operation(&database, operations[0].operation_id).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn revert_requires_the_current_renamed_filename() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("first.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);
        rename_photo(&database, photo.photo_id, "second.jpg").unwrap();
        let first_operation_id =
            list_photo_operations(&database, None, 10).unwrap().items[0].operation_id;
        rename_photo(&database, photo.photo_id, "third.jpg").unwrap();
        let second_operation_id =
            list_photo_operations(&database, None, 10).unwrap().items[0].operation_id;
        let newest_operation = list_photo_operations(&database, None, 1).unwrap();
        assert_eq!(newest_operation.items[0].operation_id, second_operation_id);
        assert!(newest_operation.next_cursor.is_some());
        let older_operation =
            list_photo_operations(&database, newest_operation.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(older_operation.items[0].operation_id, first_operation_id);
        assert_eq!(older_operation.next_cursor, None);
        let error = revert_photo_operation(&database, first_operation_id).unwrap_err();
        assert!(error.to_string().contains("expected 'second.jpg'"));
        assert!(root.path().join("third.jpg").is_file());

        revert_photo_operation(&database, second_operation_id).unwrap();
        assert!(root.path().join("second.jpg").is_file());
        revert_photo_operation(&database, first_operation_id).unwrap();
        assert!(root.path().join("first.jpg").is_file());
    }

    #[test]
    fn groups_taxon_renames_as_one_operation() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("first.jpg"), b"first").unwrap();
        fs::write(root.path().join("second.png"), b"second").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photos = list_photos(&database).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 'sci_name', 'Canis lupus')
                "#,
                [taxon_id],
            )
            .unwrap();
        for photo in &photos {
            connection
                .execute(
                    r#"
                    INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                    VALUES (?, ?, 'matched')
                    ON CONFLICT(photo_id) DO UPDATE
                    SET taxon_id = excluded.taxon_id, status = 'matched'
                    "#,
                    params![photo.photo_id, taxon_id],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
                    [photo.photo_id],
                )
                .unwrap();
        }
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (?, ?, ?)
                "#,
                params![taxon_id, photos.len() as i64, photos.len() as i64],
            )
            .unwrap();
        drop(connection);
        let photo_ids = photos
            .iter()
            .map(|photo| photo.photo_id)
            .collect::<Vec<_>>();

        let renamed = rename_photos_from_taxa(&database, &photo_ids).unwrap();

        let mut renamed_filenames = renamed
            .rows
            .iter()
            .map(|row| row.photo.as_ref().unwrap().filename.as_str())
            .collect::<Vec<_>>();
        renamed_filenames.sort_unstable();
        assert_eq!(renamed_filenames, ["Canis lupus.jpg", "Canis lupus.png"]);
        assert!(
            renamed
                .rows
                .iter()
                .all(|row| row.status == PhotoRenameRowStatus::Applied)
        );
        let operation = list_photo_operations(&database, None, 10)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(renamed.operation_id, Some(operation.operation_id));
        assert_eq!(operation.source, PhotoOperationSource::TaxonSelectionRename);
        assert_eq!(operation.items.len(), 2);
        assert_eq!(operation.items[0].row_number, 1);
        assert_eq!(operation.items[1].row_number, 2);
        let exported = export_photo_operation_inputs(&database, &[operation.operation_id]).unwrap();
        assert_eq!(exported.rows.len(), 2);

        revert_photo_operation(&database, operation.operation_id).unwrap();
        assert!(root.path().join("first.jpg").is_file());
        assert!(root.path().join("second.png").is_file());
    }

    #[test]
    fn directory_taxon_rename_selects_only_current_mappings() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("mapped.jpg"), b"mapped").unwrap();
        fs::write(root.path().join("unmapped.png"), b"unmapped").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let mut photos = list_photos(&database).unwrap();
        photos.sort_by(|left, right| left.filename.cmp(&right.filename));
        let mapped_photo = &photos[0];
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 'sci_name', 'Canis lupus')
                "#,
                [taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (?, ?, 'matched')
                "#,
                params![mapped_photo.photo_id, taxon_id],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
                [mapped_photo.photo_id],
            )
            .unwrap();
        drop(connection);

        let result =
            rename_photos_in_directory_from_taxa(&database, library.root_directory_id, false)
                .unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].photo_id, mapped_photo.photo_id);
        assert_eq!(result.rows[0].status, PhotoRenameRowStatus::Applied);
        assert!(root.path().join("Canis lupus.jpg").is_file());
        assert!(root.path().join("unmapped.png").is_file());
    }

    #[test]
    fn continues_taxon_selection_rename_after_a_row_fails() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("first.jpg"), b"first").unwrap();
        fs::write(root.path().join("second.jpg"), b"second").unwrap();
        fs::write(root.path().join("third.png"), b"third").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let mut photos = list_photos(&database).unwrap();
        photos.sort_by(|left, right| left.filename.cmp(&right.filename));
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 'sci_name', 'Canis lupus')
                "#,
                [taxon_id],
            )
            .unwrap();
        for photo in &photos {
            connection
                .execute(
                    r#"
                    INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                    VALUES (?, ?, 'matched')
                    ON CONFLICT(photo_id) DO UPDATE
                    SET taxon_id = excluded.taxon_id, status = 'matched'
                    "#,
                    params![photo.photo_id, taxon_id],
                )
                .unwrap();
            connection
                .execute(
                    "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
                    [photo.photo_id],
                )
                .unwrap();
        }
        drop(connection);
        let photo_ids = photos
            .iter()
            .map(|photo| photo.photo_id)
            .collect::<Vec<_>>();

        let result = rename_photos_from_taxa(&database, &photo_ids).unwrap();

        assert!(result.operation_id.is_some());
        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.rows[0].status, PhotoRenameRowStatus::Applied);
        assert!(result.rows[0].operation_id.is_some());
        assert_eq!(result.rows[1].status, PhotoRenameRowStatus::Failed);
        assert!(result.rows[1].operation_id.is_none());
        assert!(
            result.rows[1]
                .message
                .contains("rename destination already exists")
        );
        assert_eq!(result.rows[2].status, PhotoRenameRowStatus::Applied);
        assert!(result.rows[2].operation_id.is_some());
        let operation = get_photo_operation(&database, result.operation_id.unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            operation
                .items
                .iter()
                .map(|item| item.row_number)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert!(root.path().join("Canis lupus.jpg").is_file());
        assert!(root.path().join("second.jpg").is_file());
        assert!(root.path().join("Canis lupus.png").is_file());
    }

    #[test]
    fn renames_a_matched_photo_with_its_accepted_scientific_name() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("canis lupus.JPG"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 'sci_name', 'Canis lupus')
                "#,
                [taxon_id],
            )
            .unwrap();
        drop(connection);
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        mapping::process_pending_photo_matches(&database, &mut progress).unwrap();
        mapping::select_photo_taxon(&database, photo.photo_id, taxon_id).unwrap();
        mapping::refresh_after_taxonomy_changes(&database, [taxon_id]).unwrap();
        let error = rename_photo_from_taxon(&database, photo.photo_id).unwrap_err();
        assert!(error.to_string().contains("must have a matched taxon"));
        mapping::process_pending_photo_matches(&database, &mut progress).unwrap();

        let renamed = rename_photo_from_taxon(&database, photo.photo_id).unwrap();

        assert_eq!(renamed.filename, "Canis lupus.JPG");
        let filenames = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(filenames, ["Canis lupus.JPG"]);
    }

    #[test]
    fn restores_case_only_rename_when_the_database_update_fails() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ABC.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_photo_rename
                BEFORE UPDATE OF filename ON photos BEGIN
                    SELECT RAISE(ABORT, 'forced photo rename failure');
                END;
                "#,
            )
            .unwrap();

        let error = rename_photo(&database, photo.photo_id, "abc.jpg").unwrap_err();
        assert!(error.to_string().contains("forced photo rename failure"));
        assert_eq!(
            get_photo(&database, photo.photo_id)
                .unwrap()
                .unwrap()
                .filename,
            "ABC.jpg"
        );
        let filenames = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert!(filenames.contains(&"ABC.jpg".to_string()));
        assert!(!filenames.contains(&"abc.jpg".to_string()));
        assert!(
            !root
                .path()
                .join(format!(".vividarium-rename-{}.tmp", photo.photo_id))
                .exists()
        );
    }

    #[test]
    fn restores_case_only_revert_when_the_database_update_fails() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ABC.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = list_photos(&database).unwrap().remove(0);
        rename_photo(&database, photo.photo_id, "abc.jpg").unwrap();
        let operation_id = list_photo_operations(&database, None, 1).unwrap().items[0].operation_id;
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_photo_revert
                BEFORE UPDATE OF filename ON photos
                WHEN new.filename = 'ABC.jpg' BEGIN
                    SELECT RAISE(ABORT, 'forced photo revert failure');
                END;
                "#,
            )
            .unwrap();

        let error = revert_photo_operation(&database, operation_id).unwrap_err();

        assert!(error.to_string().contains("forced photo revert failure"));
        assert_eq!(
            get_photo(&database, photo.photo_id)
                .unwrap()
                .unwrap()
                .filename,
            "abc.jpg"
        );
        let filenames = fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        assert!(filenames.contains(&"abc.jpg".to_string()));
        assert!(!filenames.contains(&"ABC.jpg".to_string()));
        assert!(
            !root
                .path()
                .join(format!(".vividarium-revert-{operation_id}.tmp"))
                .exists()
        );
    }
}
