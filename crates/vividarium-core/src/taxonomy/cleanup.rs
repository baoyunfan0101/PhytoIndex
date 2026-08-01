use std::fs;
use std::path::Path;

use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};

use crate::metadata::{self, MetadataKey};
use crate::{CoreResult, Database};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingFileCleanup {
    path: String,
    context: String,
}

pub(super) fn retry_pending(database: &Database) -> Vec<String> {
    match retry_pending_inner(database) {
        Ok(warnings) => warnings,
        Err(error) => vec![format!("pending file cleanup retry failed: {error}")],
    }
}

pub(super) fn remove_or_defer(database: &Database, path: &Path, context: &str) -> Option<String> {
    if !path.exists() {
        return None;
    }
    match fs::remove_file(path) {
        Ok(()) => None,
        Err(_) if !path.exists() => None,
        Err(error) => {
            let warning = format!("{context} cleanup failed: {error}");
            match defer(database, path, context) {
                Ok(()) => Some(format!("{warning}; cleanup was queued for retry")),
                Err(defer_error) => Some(format!(
                    "{warning}; cleanup could not be queued: {defer_error}"
                )),
            }
        }
    }
}

fn retry_pending_inner(database: &Database) -> CoreResult<Vec<String>> {
    let mut connection = database.connect_metadata()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let pending = metadata::get_json::<Vec<PendingFileCleanup>>(
        &transaction,
        MetadataKey::PendingFileCleanup,
    )?
    .unwrap_or_default();
    let mut remaining = Vec::new();
    let mut warnings = Vec::new();
    for entry in pending {
        let path = Path::new(&entry.path);
        if !path.exists() {
            continue;
        }
        if let Err(error) = fs::remove_file(path)
            && path.exists()
        {
            warnings.push(format!("{} cleanup retry failed: {error}", entry.context));
            remaining.push(entry);
        }
    }
    metadata::set_json(&transaction, MetadataKey::PendingFileCleanup, &remaining)?;
    transaction.commit()?;
    Ok(warnings)
}

fn defer(database: &Database, path: &Path, context: &str) -> CoreResult<()> {
    let mut connection = database.connect_metadata()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut pending = metadata::get_json::<Vec<PendingFileCleanup>>(
        &transaction,
        MetadataKey::PendingFileCleanup,
    )?
    .unwrap_or_default();
    let path = path.to_string_lossy().into_owned();
    if let Some(entry) = pending.iter_mut().find(|entry| entry.path == path) {
        entry.context = context.to_string();
    } else {
        pending.push(PendingFileCleanup {
            path,
            context: context.to_string(),
        });
    }
    metadata::set_json(&transaction, MetadataKey::PendingFileCleanup, &pending)?;
    transaction.commit()?;
    Ok(())
}
