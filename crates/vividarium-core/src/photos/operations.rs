use std::path::PathBuf;

use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{
    PHOTO_WRITE_LOCK, get_photo, library_root, load_directory, rename_file, safe_directory_path,
};
use crate::mapping;
use crate::operations::{
    self, NewAuditRow, NewOperation, OperationAuditRow, OperationPage, OperationSummary,
};
use crate::{CoreError, CoreResult, Database};

const PHOTO_RENAME_KIND: &str = "photo_rename";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum PhotoOperationSource {
    Manual,
    Taxon,
    TaxonSelection,
}

impl PhotoOperationSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual_rename",
            Self::Taxon => "taxon_rename",
            Self::TaxonSelection => "taxon_selection_rename",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PhotoRenameState {
    directory_relative_path: String,
    filename: String,
}

#[derive(Debug, Clone)]
struct PhotoOperationItem {
    sequence: usize,
    photo_id: i64,
    before: PhotoRenameState,
    after: PhotoRenameState,
}

pub(super) fn start_photo_operation(
    database: &Database,
    source: PhotoOperationSource,
    total_items: usize,
) -> CoreResult<i64> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: PHOTO_RENAME_KIND,
            source: source.as_str(),
            total_items,
            succeeded_items: 0,
            failed_items: total_items,
            rollbackable: true,
            has_formatted_input: false,
        },
    )?;
    transaction.commit()?;
    Ok(operation_id)
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
    operations::insert_audit_row(
        transaction,
        operation_id,
        NewAuditRow {
            sequence: row_number,
            entity_type: "photo",
            entity_id: Some(photo_id.to_string()),
            action: "rename",
            before_json: Some(json!({
                "directory_relative_path": directory_relative_path,
                "filename": old_filename,
            })),
            after_json: Some(json!({
                "directory_relative_path": directory_relative_path,
                "filename": new_filename,
            })),
            succeeded: true,
            message: "renamed",
        },
    )?;
    let (total_items, succeeded_items) = transaction.query_row(
        r#"
        SELECT operations.total_items, COUNT(operation_audit_rows.sequence)
        FROM operations
        LEFT JOIN operation_audit_rows USING (operation_id)
        WHERE operations.operation_id = ?
          AND operation_audit_rows.succeeded = 1
        "#,
        [operation_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get::<_, i64>(1)? as usize,
            ))
        },
    )?;
    operations::update_operation_counts(
        transaction,
        operation_id,
        total_items,
        succeeded_items,
        total_items.saturating_sub(succeeded_items),
    )
}

pub(super) fn record_operation_outcomes(
    database: &Database,
    operation_id: Option<i64>,
    rows: &[(usize, i64, &'static str, bool, String)],
) -> CoreResult<()> {
    let Some(operation_id) = operation_id else {
        return Ok(());
    };
    let mut connection = database.connect()?;
    let transaction = connection.transaction()?;
    for (sequence, photo_id, action, succeeded, message) in rows {
        operations::insert_audit_row(
            &transaction,
            operation_id,
            NewAuditRow {
                sequence: *sequence,
                entity_type: "photo",
                entity_id: Some(photo_id.to_string()),
                action,
                before_json: None,
                after_json: None,
                succeeded: *succeeded,
                message,
            },
        )?;
    }
    let (total_items, succeeded_items, failed_items) = transaction.query_row(
        r#"
        SELECT operations.total_items,
               COALESCE(SUM(operation_audit_rows.succeeded = 1), 0),
               COALESCE(SUM(operation_audit_rows.succeeded = 0), 0)
        FROM operations
        LEFT JOIN operation_audit_rows USING (operation_id)
        WHERE operations.operation_id = ?
        "#,
        [operation_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get::<_, i64>(1)? as usize,
                row.get::<_, i64>(2)? as usize,
            ))
        },
    )?;
    operations::update_operation_counts(
        &transaction,
        operation_id,
        total_items,
        succeeded_items,
        failed_items,
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn list_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<OperationPage<OperationSummary>> {
    operations::list_operations(&database.connect()?, cursor, limit)
}

pub fn list_operation_audit(
    database: &Database,
    operation_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<OperationPage<OperationAuditRow>> {
    operations::list_operation_audit(&database.connect()?, operation_id, cursor, limit)
}

pub fn write_operation_audit<W: std::io::Write>(
    database: &Database,
    operation_id: i64,
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect()?,
        Some(&[operation_id]),
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn write_operations_audit<W: std::io::Write>(
    database: &Database,
    operation_ids: &[i64],
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect()?,
        Some(operation_ids),
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn write_all_operation_audit<W: std::io::Write>(
    database: &Database,
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect()?,
        None,
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn rollback_operation(database: &Database, operation_id: i64) -> CoreResult<()> {
    let _guard = PHOTO_WRITE_LOCK
        .lock()
        .map_err(|_| CoreError::InvalidArgument("photo workspace lock is poisoned".into()))?;
    let connection = database.connect()?;
    let summary = operations::get_operation(&connection, operation_id)?
        .ok_or_else(|| CoreError::NotFound(format!("operation {operation_id}")))?;
    if summary.kind != PHOTO_RENAME_KIND || !summary.rollbackable {
        return Err(CoreError::InvalidArgument(format!(
            "operation {operation_id} cannot be rolled back"
        )));
    }
    let items = load_operation_items(&connection, operation_id)?;
    let root = library_root(&connection)?;
    let mut revert_items = Vec::with_capacity(items.len());
    for item in items {
        let photo = get_photo(database, item.photo_id)?
            .ok_or_else(|| CoreError::NotFound(format!("photo {}", item.photo_id)))?;
        if photo.filename != item.after.filename {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} filename is '{}', expected '{}'",
                item.photo_id, photo.filename, item.after.filename
            )));
        }
        let directory = load_directory(&connection, photo.directory_id)?.ok_or_else(|| {
            CoreError::NotFound(format!("photo directory {}", photo.directory_id))
        })?;
        if directory.relative_path != item.after.directory_relative_path {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} is no longer in the recorded directory",
                item.photo_id
            )));
        }
        let directory_path = safe_directory_path(&root, &directory.relative_path)?;
        let source = directory_path.join(&item.after.filename);
        let destination = directory_path.join(&item.before.filename);
        let temporary = directory_path.join(format!(
            ".vividarium-revert-{operation_id}-{}.tmp",
            item.sequence
        ));
        revert_items.push(RevertItem {
            item,
            directory_id: photo.directory_id,
            source,
            destination,
            temporary,
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
    let result = rollback_operation_database(database, operation_id, &revert_items);
    match result {
        Ok(()) => Ok(()),
        Err(error) => match rollback_reverted_files(&renamed) {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(CoreError::Consistency(format!(
                "photo operation rollback failed: {error}; filesystem rollback failed: {rollback_error}"
            ))),
        },
    }
}

fn load_operation_items(
    connection: &rusqlite::Connection,
    operation_id: i64,
) -> CoreResult<Vec<PhotoOperationItem>> {
    let mut statement = connection.prepare(
        r#"
        SELECT sequence, entity_id, before_json, after_json
        FROM operation_audit_rows
        WHERE operation_id = ? AND entity_type = 'photo'
          AND action = 'rename' AND succeeded = 1
        ORDER BY sequence
        "#,
    )?;
    let stored = statement
        .query_map([operation_id], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    stored
        .into_iter()
        .map(|(sequence, photo_id, before, after)| {
            Ok(PhotoOperationItem {
                sequence,
                photo_id: photo_id.parse().map_err(|_| {
                    CoreError::Consistency(format!(
                        "operation {operation_id} has an invalid photo id"
                    ))
                })?,
                before: serde_json::from_str(&before).map_err(invalid_photo_audit)?,
                after: serde_json::from_str(&after).map_err(invalid_photo_audit)?,
            })
        })
        .collect()
}

fn rollback_operation_database(
    database: &Database,
    operation_id: i64,
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
                item.item.before.filename,
                item.item.photo_id,
                item.directory_id,
                item.item.after.filename
            ],
        )?;
        if updated != 1 {
            return Err(CoreError::InvalidArgument(format!(
                "photo {} no longer matches operation {}",
                item.item.photo_id, operation_id
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
    operations::delete_operation(&transaction, operation_id)?;
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

fn invalid_photo_audit(error: serde_json::Error) -> CoreError {
    CoreError::Consistency(format!("invalid photo operation audit: {error}"))
}
