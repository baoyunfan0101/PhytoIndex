use rusqlite::{OptionalExtension, Transaction, params};
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
use crate::models::PhotoPage;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoOperationSource {
    ManualRename,
    TaxonRename,
    TaxonBatchRename,
}

impl PhotoOperationSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::ManualRename => "manual_rename",
            Self::TaxonRename => "taxon_rename",
            Self::TaxonBatchRename => "taxon_batch_rename",
        }
    }

    fn from_str(value: &str) -> CoreResult<Self> {
        match value {
            "manual_rename" => Ok(Self::ManualRename),
            "taxon_rename" => Ok(Self::TaxonRename),
            "taxon_batch_rename" => Ok(Self::TaxonBatchRename),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid photo operation source: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoOperationStatus {
    Applied,
    Reverted,
}

impl PhotoOperationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Reverted => "reverted",
        }
    }

    fn from_str(value: &str) -> CoreResult<Self> {
        match value {
            "applied" => Ok(Self::Applied),
            "reverted" => Ok(Self::Reverted),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid photo operation status: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoOperationBatch {
    pub batch_id: i64,
    pub source: PhotoOperationSource,
    pub root_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoOperation {
    pub operation_id: i64,
    pub batch_id: i64,
    pub row_number: usize,
    pub status: PhotoOperationStatus,
    pub photo_id: i64,
    pub directory_relative_path: String,
    pub old_filename: String,
    pub new_filename: String,
    pub applied_at: String,
    pub reverted_at: Option<String>,
}

pub(super) fn insert_photo_operation_batch(
    transaction: &Transaction<'_>,
    source: PhotoOperationSource,
    root_path: &str,
) -> CoreResult<i64> {
    transaction.execute(
        r#"
        INSERT INTO photo_operation_batches (source, root_path)
        VALUES (?, ?)
        "#,
        params![source.as_str(), root_path],
    )?;
    Ok(transaction.last_insert_rowid())
}

pub(super) fn insert_photo_operation(
    transaction: &Transaction<'_>,
    batch_id: i64,
    row_number: usize,
    photo_id: i64,
    directory_relative_path: &str,
    old_filename: &str,
    new_filename: &str,
) -> CoreResult<i64> {
    transaction.execute(
        r#"
        INSERT INTO photo_operations (
            batch_id, row_number, status, photo_id,
            directory_relative_path, old_filename, new_filename
        ) VALUES (?, ?, 'applied', ?, ?, ?, ?)
        "#,
        params![
            batch_id,
            row_number as i64,
            photo_id,
            directory_relative_path,
            old_filename,
            new_filename
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

pub fn list_photo_operation_batches(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperationBatch>> {
    let connection = database.connect()?;
    let batch_cursor = match decode_photo_cursor(cursor)? {
        None => None,
        Some(PhotoCursor::OperationBatches {
            created_at,
            batch_id,
        }) => Some((created_at, batch_id)),
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let mut items = if let Some((created_at, batch_id)) = batch_cursor {
        let mut statement = connection.prepare(
            r#"
            SELECT batch_id, source, root_path, created_at
            FROM photo_operation_batches
            WHERE (created_at, batch_id) < (?1, ?2)
            ORDER BY created_at DESC, batch_id DESC
            LIMIT ?3
            "#,
        )?;
        let rows = statement.query_map(
            params![created_at, batch_id, fetch_limit as i64],
            photo_operation_batch_row,
        )?;
        rows.map(photo_operation_batch_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    } else {
        let mut statement = connection.prepare(
            r#"
            SELECT batch_id, source, root_path, created_at
            FROM photo_operation_batches
            ORDER BY created_at DESC, batch_id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([fetch_limit as i64], photo_operation_batch_row)?;
        rows.map(photo_operation_batch_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    };
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items
            .last()
            .map(|batch| {
                encode_photo_cursor(&PhotoCursor::OperationBatches {
                    created_at: batch.created_at.clone(),
                    batch_id: batch.batch_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
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
    let mut items = if let Some(operation_id) = operation_cursor {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, batch_id, row_number, status, photo_id,
                   directory_relative_path, old_filename, new_filename,
                   applied_at, reverted_at
            FROM photo_operations
            WHERE operation_id < ?1
            ORDER BY operation_id DESC
            LIMIT ?2
            "#,
        )?;
        let rows = statement.query_map(
            params![operation_id, fetch_limit as i64],
            photo_operation_row,
        )?;
        rows.map(photo_operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    } else {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, batch_id, row_number, status, photo_id,
                   directory_relative_path, old_filename, new_filename,
                   applied_at, reverted_at
            FROM photo_operations
            ORDER BY operation_id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([fetch_limit as i64], photo_operation_row)?;
        rows.map(photo_operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    };
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items
            .last()
            .map(|operation| {
                encode_photo_cursor(&PhotoCursor::Operations {
                    operation_id: operation.operation_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

pub fn list_photo_operations_for_batch(
    database: &Database,
    batch_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoOperation>> {
    let connection = database.connect()?;
    let operation_cursor = match decode_photo_cursor(cursor)? {
        None => None,
        Some(PhotoCursor::BatchOperations {
            batch_id: cursor_batch_id,
            row_number,
            operation_id,
        }) if cursor_batch_id == batch_id => Some((row_number, operation_id)),
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let mut items = if let Some((row_number, operation_id)) = operation_cursor {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, batch_id, row_number, status, photo_id,
                   directory_relative_path, old_filename, new_filename,
                   applied_at, reverted_at
            FROM photo_operations
            WHERE batch_id = ?1 AND (row_number, operation_id) > (?2, ?3)
            ORDER BY row_number, operation_id
            LIMIT ?4
            "#,
        )?;
        let rows = statement.query_map(
            params![
                batch_id,
                row_number as i64,
                operation_id,
                fetch_limit as i64
            ],
            photo_operation_row,
        )?;
        rows.map(photo_operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    } else {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, batch_id, row_number, status, photo_id,
                   directory_relative_path, old_filename, new_filename,
                   applied_at, reverted_at
            FROM photo_operations
            WHERE batch_id = ?1
            ORDER BY row_number, operation_id
            LIMIT ?2
            "#,
        )?;
        let rows =
            statement.query_map(params![batch_id, fetch_limit as i64], photo_operation_row)?;
        rows.map(photo_operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    };
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items
            .last()
            .map(|operation| {
                encode_photo_cursor(&PhotoCursor::BatchOperations {
                    batch_id,
                    row_number: operation.row_number,
                    operation_id: operation.operation_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

pub fn revert_photo_operation(database: &Database, operation_id: i64) -> CoreResult<()> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let connection = database.connect()?;
    let (
        status,
        photo_id,
        directory_relative_path,
        old_filename,
        new_filename,
        logged_root_path,
    ): (String, i64, String, String, String, String) = connection
        .query_row(
            r#"
            SELECT photo_operations.status, photo_operations.photo_id,
                   photo_operations.directory_relative_path,
                   photo_operations.old_filename, photo_operations.new_filename,
                   photo_operation_batches.root_path
            FROM photo_operations
            JOIN photo_operation_batches USING (batch_id)
            WHERE photo_operations.operation_id = ?
            "#,
            [operation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("photo operation {operation_id}")))?;
    let status = PhotoOperationStatus::from_str(&status)?;
    if status != PhotoOperationStatus::Applied {
        return Err(CoreError::InvalidArgument(format!(
            "photo operation {operation_id} is already {}",
            status.as_str()
        )));
    }
    let root = library_root(&connection)?;
    if root.to_str() != Some(logged_root_path.as_str()) {
        return Err(CoreError::InvalidArgument(format!(
            "photo operation {operation_id} belongs to another photo library"
        )));
    }
    let photo = get_photo(database, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    if photo.filename != new_filename {
        return Err(CoreError::InvalidArgument(format!(
            "photo {photo_id} filename is '{}', expected '{}'",
            photo.filename, new_filename
        )));
    }
    let directory = load_directory(&connection, photo.directory_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo directory {}", photo.directory_id)))?;
    if directory.relative_path != directory_relative_path {
        return Err(CoreError::InvalidArgument(format!(
            "photo {photo_id} is no longer in the recorded directory"
        )));
    }
    let directory_path = safe_directory_path(&root, &directory_relative_path)?;
    let source = directory_path.join(&new_filename);
    let destination = directory_path.join(&old_filename);
    let temporary = directory_path.join(format!(".vividarium-revert-{operation_id}.tmp"));
    rename_file(&source, &destination, &temporary)?;

    let result = (|| -> CoreResult<()> {
        let mut connection = database.connect()?;
        let transaction = connection.transaction()?;
        let updated = transaction.execute(
            r#"
            UPDATE photos
            SET filename = ?
            WHERE photo_id = ? AND directory_id = ? AND filename = ?
            "#,
            params![old_filename, photo_id, photo.directory_id, new_filename],
        )?;
        if updated != 1 {
            return Err(CoreError::InvalidArgument(format!(
                "photo {photo_id} no longer matches operation {operation_id}"
            )));
        }
        mapping::remap_photo_ids(&transaction, &[photo_id])?;
        transaction.execute(
            "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
            [photo_id],
        )?;
        let updated = transaction.execute(
            r#"
            UPDATE photo_operations
            SET status = 'reverted', reverted_at = CURRENT_TIMESTAMP
            WHERE operation_id = ? AND status = 'applied'
            "#,
            [operation_id],
        )?;
        if updated != 1 {
            return Err(CoreError::InvalidArgument(format!(
                "photo operation {operation_id} is no longer applied"
            )));
        }
        transaction.commit()?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => match rename_file(&destination, &source, &temporary) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "photo operation revert failed: {error}; filesystem rollback failed: {rollback_error}"
            ))),
        },
    }
}

fn photo_operation_batch_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(i64, String, String, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn photo_operation_batch_from_row(
    row: rusqlite::Result<(i64, String, String, String)>,
) -> CoreResult<PhotoOperationBatch> {
    let (batch_id, source, root_path, created_at) = row?;
    Ok(PhotoOperationBatch {
        batch_id,
        source: PhotoOperationSource::from_str(&source)?,
        root_path,
        created_at,
    })
}

type PhotoOperationRow = (
    i64,
    i64,
    i64,
    String,
    i64,
    String,
    String,
    String,
    String,
    Option<String>,
);

fn photo_operation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoOperationRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn photo_operation_from_row(
    row: rusqlite::Result<PhotoOperationRow>,
) -> CoreResult<PhotoOperation> {
    let (
        operation_id,
        batch_id,
        row_number,
        status,
        photo_id,
        directory_relative_path,
        old_filename,
        new_filename,
        applied_at,
        reverted_at,
    ) = row?;
    Ok(PhotoOperation {
        operation_id,
        batch_id,
        row_number: row_number as usize,
        status: PhotoOperationStatus::from_str(&status)?,
        photo_id,
        directory_relative_path,
        old_filename,
        new_filename,
        applied_at,
        reverted_at,
    })
}
