use std::collections::BTreeSet;
use std::path::PathBuf;

use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::page::{
    PhotoCursor, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor, photo_page_limit,
};
use super::{
    PHOTO_WRITE_LOCK, get_photo, library_root, load_directory, rename_file, safe_directory_path,
};
use crate::db::Database;
use crate::error::{CoreError, CoreResult};
use crate::mapping;
use crate::models::{OperationInputTable, PhotoPage};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoOperationSource {
    ManualRename,
    TaxonRename,
    TaxonSelectionRename,
}

impl PhotoOperationSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::ManualRename => "manual_rename",
            Self::TaxonRename => "taxon_rename",
            Self::TaxonSelectionRename => "taxon_selection_rename",
        }
    }

    fn from_str(value: &str) -> CoreResult<Self> {
        match value {
            "manual_rename" => Ok(Self::ManualRename),
            "taxon_rename" => Ok(Self::TaxonRename),
            "taxon_selection_rename" => Ok(Self::TaxonSelectionRename),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid photo operation source: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoOperationInput {
    pub row_number: usize,
    pub photo_id: i64,
    pub requested_filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoOperationItem {
    pub row_number: usize,
    pub photo_id: i64,
    pub directory_relative_path: String,
    pub old_filename: String,
    pub new_filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoOperation {
    pub operation_id: i64,
    pub source: PhotoOperationSource,
    pub root_path: String,
    pub input: Vec<PhotoOperationInput>,
    pub items: Vec<PhotoOperationItem>,
    pub applied_at: String,
}

pub(super) fn insert_photo_operation(
    transaction: &Transaction<'_>,
    source: PhotoOperationSource,
    root_path: &str,
    input: &[PhotoOperationInput],
) -> CoreResult<i64> {
    let input_json = serde_json::to_string(input)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid photo input: {error}")))?;
    transaction.execute(
        r#"
        INSERT INTO photo_operations (source, root_path, input_json)
        VALUES (?, ?, ?)
        "#,
        params![source.as_str(), root_path, input_json],
    )?;
    Ok(transaction.last_insert_rowid())
}

pub(super) fn insert_photo_operation_item(
    transaction: &Transaction<'_>,
    operation_id: i64,
    row_number: usize,
    photo_id: i64,
    directory_relative_path: &str,
    old_filename: &str,
    new_filename: &str,
) -> CoreResult<()> {
    transaction.execute(
        r#"
        INSERT INTO photo_operation_items (
            operation_id, row_number, photo_id,
            directory_relative_path, old_filename, new_filename
        ) VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            operation_id,
            row_number as i64,
            photo_id,
            directory_relative_path,
            old_filename,
            new_filename
        ],
    )?;
    Ok(())
}

pub fn list_photo_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperation>> {
    let connection = database.connect()?;
    let operation_cursor = match decode_photo_cursor(cursor)? {
        None => None,
        Some(PhotoCursor::Operations { operation_id }) => Some(operation_id),
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let mut headers = if let Some(operation_id) = operation_cursor {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, source, root_path, input_json, applied_at
            FROM photo_operations
            WHERE operation_id < ?1
            ORDER BY operation_id DESC
            LIMIT ?2
            "#,
        )?;
        statement
            .query_map(
                params![operation_id, fetch_limit as i64],
                photo_operation_header_row,
            )?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, source, root_path, input_json, applied_at
            FROM photo_operations
            ORDER BY operation_id DESC
            LIMIT ?1
            "#,
        )?;
        statement
            .query_map([fetch_limit as i64], photo_operation_header_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    let next_cursor = if headers.len() > limit {
        headers.truncate(limit);
        headers
            .last()
            .map(|header| {
                encode_photo_cursor(&PhotoCursor::Operations {
                    operation_id: header.operation_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    let items = headers
        .into_iter()
        .map(|header| photo_operation_from_header(&connection, header))
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(PhotoPage { items, next_cursor })
}

pub fn get_photo_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<Option<PhotoOperation>> {
    let connection = database.connect()?;
    connection
        .query_row(
            r#"
            SELECT operation_id, source, root_path, input_json, applied_at
            FROM photo_operations
            WHERE operation_id = ?
            "#,
            [operation_id],
            photo_operation_header_row,
        )
        .optional()?
        .map(|header| photo_operation_from_header(&connection, header))
        .transpose()
}

pub fn export_photo_operation_inputs(
    database: &Database,
    operation_ids: &[i64],
) -> CoreResult<OperationInputTable> {
    let operation_ids = unique_operation_ids(operation_ids)?;
    let mut rows = Vec::new();
    if !operation_ids.is_empty() {
        let connection = database.connect()?;
        let placeholders = std::iter::repeat_n("?", operation_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = connection.prepare(&format!(
            "SELECT input_json FROM photo_operations \
             WHERE operation_id IN ({placeholders}) ORDER BY operation_id"
        ))?;
        let inputs = statement
            .query_map(params_from_iter(operation_ids.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if inputs.len() != operation_ids.len() {
            return Err(CoreError::NotFound("one or more photo operations".into()));
        }
        for input_json in inputs {
            let inputs: Vec<PhotoOperationInput> =
                serde_json::from_str(&input_json).map_err(|error| {
                    CoreError::InvalidArgument(format!("invalid photo input: {error}"))
                })?;
            rows.extend(inputs.into_iter().map(|input| {
                vec![
                    input.photo_id.to_string(),
                    input.requested_filename.unwrap_or_default(),
                ]
            }));
        }
    }
    Ok(OperationInputTable {
        columns: vec!["photo_id".into(), "requested_filename".into()],
        rows,
    })
}

pub fn revert_photo_operation(database: &Database, operation_id: i64) -> CoreResult<()> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let operation = get_photo_operation(database, operation_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo operation {operation_id}")))?;
    let connection = database.connect()?;
    let root = library_root(&connection)?;
    if root.to_str() != Some(operation.root_path.as_str()) {
        return Err(CoreError::InvalidArgument(format!(
            "photo operation {operation_id} belongs to another photo library"
        )));
    }
    let mut revert_items = Vec::with_capacity(operation.items.len());
    for item in &operation.items {
        let photo = get_photo(database, item.photo_id)?
            .ok_or_else(|| CoreError::NotFound(format!("photo {}", item.photo_id)))?;
        if photo.filename != item.new_filename {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} filename is '{}', expected '{}'",
                item.photo_id, photo.filename, item.new_filename
            )));
        }
        let directory = load_directory(&connection, photo.directory_id)?.ok_or_else(|| {
            CoreError::NotFound(format!("photo directory {}", photo.directory_id))
        })?;
        if directory.relative_path != item.directory_relative_path {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} is no longer in the recorded directory",
                item.photo_id
            )));
        }
        let directory_path = safe_directory_path(&root, &directory.relative_path)?;
        revert_items.push(RevertItem {
            item: item.clone(),
            directory_id: photo.directory_id,
            source: directory_path.join(&item.new_filename),
            destination: directory_path.join(&item.old_filename),
            temporary: directory_path.join(format!(
                ".vividarium-revert-{operation_id}-{}.tmp",
                item.row_number
            )),
        });
    }
    let mut renamed = Vec::new();
    for item in revert_items.iter().rev() {
        if let Err(error) = rename_file(&item.source, &item.destination, &item.temporary) {
            rollback_reverted_files(&renamed)?;
            return Err(error);
        }
        renamed.push(item);
    }
    let result = revert_photo_operation_database(database, &operation, &revert_items);
    match result {
        Ok(()) => Ok(()),
        Err(error) => match rollback_reverted_files(&renamed) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "photo operation revert failed: {error}; filesystem rollback failed: {rollback_error}"
            ))),
        },
    }
}

fn revert_photo_operation_database(
    database: &Database,
    operation: &PhotoOperation,
    items: &[RevertItem],
) -> CoreResult<()> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let mut photo_ids = Vec::with_capacity(items.len());
    for item in items {
        let updated = transaction.execute(
            r#"
            UPDATE photos
            SET filename = ?
            WHERE photo_id = ? AND directory_id = ? AND filename = ?
            "#,
            params![
                item.item.old_filename,
                item.item.photo_id,
                item.directory_id,
                item.item.new_filename
            ],
        )?;
        if updated != 1 {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} no longer matches operation {}",
                item.item.photo_id, operation.operation_id
            )));
        }
        photo_ids.push(item.item.photo_id);
    }
    mapping::remap_photo_ids(&transaction, &photo_ids)?;
    for photo_id in photo_ids {
        transaction.execute(
            "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
            [photo_id],
        )?;
    }
    let deleted = transaction.execute(
        "DELETE FROM photo_operations WHERE operation_id = ?",
        [operation.operation_id],
    )?;
    if deleted != 1 {
        return Err(CoreError::InvalidArgument(format!(
            "photo operation {} is no longer applied",
            operation.operation_id
        )));
    }
    transaction.commit()?;
    Ok(())
}

fn rollback_reverted_files(items: &[&RevertItem]) -> CoreResult<()> {
    for item in items.iter().rev() {
        rename_file(&item.destination, &item.source, &item.temporary)?;
    }
    Ok(())
}

#[derive(Debug)]
struct RevertItem {
    item: PhotoOperationItem,
    directory_id: i64,
    source: PathBuf,
    destination: PathBuf,
    temporary: PathBuf,
}

#[derive(Debug)]
struct PhotoOperationHeader {
    operation_id: i64,
    source: String,
    root_path: String,
    input_json: String,
    applied_at: String,
}

fn photo_operation_header_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoOperationHeader> {
    Ok(PhotoOperationHeader {
        operation_id: row.get(0)?,
        source: row.get(1)?,
        root_path: row.get(2)?,
        input_json: row.get(3)?,
        applied_at: row.get(4)?,
    })
}

fn photo_operation_from_header(
    connection: &rusqlite::Connection,
    header: PhotoOperationHeader,
) -> CoreResult<PhotoOperation> {
    let input = serde_json::from_str(&header.input_json)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid photo input: {error}")))?;
    let mut statement = connection.prepare(
        r#"
        SELECT row_number, photo_id, directory_relative_path, old_filename, new_filename
        FROM photo_operation_items
        WHERE operation_id = ?
        ORDER BY row_number
        "#,
    )?;
    let items = statement
        .query_map([header.operation_id], |row| {
            Ok(PhotoOperationItem {
                row_number: row.get::<_, i64>(0)? as usize,
                photo_id: row.get(1)?,
                directory_relative_path: row.get(2)?,
                old_filename: row.get(3)?,
                new_filename: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PhotoOperation {
        operation_id: header.operation_id,
        source: PhotoOperationSource::from_str(&header.source)?,
        root_path: header.root_path,
        input,
        items,
        applied_at: header.applied_at,
    })
}

fn unique_operation_ids(operation_ids: &[i64]) -> CoreResult<Vec<i64>> {
    let unique = operation_ids.iter().copied().collect::<BTreeSet<_>>();
    if unique.iter().any(|operation_id| *operation_id <= 0) {
        return Err(CoreError::InvalidArgument(
            "operation ids must be positive".into(),
        ));
    }
    Ok(unique.into_iter().collect())
}
