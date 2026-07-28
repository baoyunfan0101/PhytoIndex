use std::collections::BTreeSet;
use std::sync::Mutex;

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::models::PhotoLibraryRegistration;
use crate::{CoreError, CoreResult, Database};

static SYNC_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomySyncResult {
    pub library_uuid: String,
    pub sync_id: i64,
    pub queued_photo_count: i64,
    pub full_remap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomySyncRun {
    pub synchronized: Vec<TaxonomySyncResult>,
    pub pending_library_uuids: Vec<String>,
}

pub(crate) fn record_event(
    transaction: &Transaction<'_>,
    source_operation_id: Option<i64>,
    affected_taxon_ids: impl IntoIterator<Item = i64>,
    full_remap_required: bool,
) -> CoreResult<i64> {
    let affected_taxon_ids = affected_taxon_ids.into_iter().collect::<BTreeSet<_>>();
    transaction.execute(
        r#"
        INSERT INTO taxonomy_sync_events (
            source_operation_id, full_remap_required
        ) VALUES (?, ?)
        "#,
        params![source_operation_id, full_remap_required],
    )?;
    let sync_id = transaction.last_insert_rowid();
    let mut insert = transaction.prepare_cached(
        r#"
        INSERT INTO taxonomy_sync_event_taxa (sync_id, taxon_id)
        VALUES (?, ?)
        "#,
    )?;
    for taxon_id in affected_taxon_ids {
        insert.execute(params![sync_id, taxon_id])?;
    }
    Ok(sync_id)
}

pub fn synchronize_pending_photo_libraries(database: &Database) -> CoreResult<TaxonomySyncRun> {
    let _guard = SYNC_LOCK
        .lock()
        .map_err(|_| CoreError::Consistency("taxonomy sync lock is poisoned".into()))?;
    dispatch_pending_events_unlocked(database)?;
    let active_uuid = database
        .active_photo_library()?
        .map(|library| library.library_uuid);
    let metadata = database.connect_metadata()?;
    let mut pending_statement =
        metadata.prepare("SELECT library_uuid FROM photo_library_taxonomy_pending")?;
    let pending_uuids = pending_statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut libraries = database.list_photo_libraries()?;
    libraries.retain(|library| {
        pending_uuids.contains(&library.library_uuid)
            || active_uuid.as_deref() == Some(library.library_uuid.as_str())
    });
    libraries.sort_by_key(|library| {
        (
            active_uuid.as_deref() != Some(library.library_uuid.as_str()),
            library.last_opened_at.clone(),
        )
    });
    let mut synchronized = Vec::new();
    let mut pending_library_uuids = Vec::new();
    for library in libraries {
        match synchronize_photo_library_unlocked(database, &library) {
            Ok(result) => synchronized.push(result),
            Err(_) => pending_library_uuids.push(library.library_uuid),
        }
    }
    Ok(TaxonomySyncRun {
        synchronized,
        pending_library_uuids,
    })
}

pub(crate) fn dispatch_pending_events(database: &Database) -> CoreResult<()> {
    let _guard = SYNC_LOCK
        .lock()
        .map_err(|_| CoreError::Consistency("taxonomy sync lock is poisoned".into()))?;
    dispatch_pending_events_unlocked(database)
}

fn dispatch_pending_events_unlocked(database: &Database) -> CoreResult<()> {
    let taxonomy = database.connect_taxonomy()?;
    let metadata = database.connect_metadata()?;
    let last_dispatched = metadata.query_row(
        r#"
        SELECT last_dispatched_sync_id
        FROM taxonomy_sync_dispatch
        WHERE dispatch_id = 1
        "#,
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let mut event_statement = taxonomy.prepare(
        r#"
        SELECT sync_id, full_remap_required
        FROM taxonomy_sync_events
        WHERE sync_id > ?
        ORDER BY sync_id
        "#,
    )?;
    let events = event_statement
        .query_map([last_dispatched], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if events.is_empty() {
        return Ok(());
    }
    let latest_sync_id = events
        .last()
        .map(|event| event.0)
        .unwrap_or(last_dispatched);
    let full_remap_required = events.iter().any(|event| event.1);
    let mut affected_taxon_ids = BTreeSet::<i64>::new();
    if !full_remap_required {
        let mut statement = taxonomy.prepare(
            r#"
            SELECT DISTINCT taxon_id
            FROM taxonomy_sync_event_taxa
            WHERE sync_id > ? AND sync_id <= ?
            "#,
        )?;
        affected_taxon_ids = statement
            .query_map(params![last_dispatched, latest_sync_id], |row| row.get(0))?
            .collect::<Result<BTreeSet<_>, _>>()?;
    }
    let mut metadata = metadata;
    let transaction = metadata.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        r#"
        INSERT INTO photo_library_taxonomy_pending (
            library_uuid, target_sync_id, full_remap_required
        )
        SELECT library_uuid, ?, ?
        FROM photo_libraries
        WHERE true
        ON CONFLICT(library_uuid) DO UPDATE SET
            target_sync_id = max(target_sync_id, excluded.target_sync_id),
            full_remap_required = max(
                full_remap_required,
                excluded.full_remap_required
            )
        "#,
        params![latest_sync_id, full_remap_required],
    )?;
    if full_remap_required {
        transaction.execute("DELETE FROM photo_library_taxonomy_pending_taxa", [])?;
    } else {
        let mut insert = transaction.prepare_cached(
            r#"
            INSERT INTO photo_library_taxonomy_pending_taxa (
                library_uuid, taxon_id
            )
            SELECT pending.library_uuid, ?
            FROM photo_library_taxonomy_pending AS pending
            WHERE pending.full_remap_required = 0
            ON CONFLICT(library_uuid, taxon_id) DO NOTHING
            "#,
        )?;
        for taxon_id in affected_taxon_ids {
            insert.execute([taxon_id])?;
        }
    }
    transaction.execute(
        r#"
        UPDATE taxonomy_sync_dispatch
        SET last_dispatched_sync_id = ?
        WHERE dispatch_id = 1
        "#,
        [latest_sync_id],
    )?;
    transaction.commit()?;
    Ok(())
}

pub(crate) fn synchronize_photo_library(
    database: &Database,
    library: &PhotoLibraryRegistration,
) -> CoreResult<TaxonomySyncResult> {
    let _guard = SYNC_LOCK
        .lock()
        .map_err(|_| CoreError::Consistency("taxonomy sync lock is poisoned".into()))?;
    dispatch_pending_events_unlocked(database)?;
    synchronize_photo_library_unlocked(database, library)
}

fn synchronize_photo_library_unlocked(
    database: &Database,
    library: &PhotoLibraryRegistration,
) -> CoreResult<TaxonomySyncResult> {
    let taxonomy_identity = database.taxonomy_identity()?;
    let mut connection = database.connect_photo_library_registration(library)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (bound_identity, last_sync_id) = transaction.query_row(
        r#"
        SELECT bound_taxonomy_identity, last_taxonomy_sync_id
        FROM photo_library
        WHERE library_id = 1
        "#,
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let pending = transaction
        .query_row(
            r#"
            SELECT target_sync_id, full_remap_required
            FROM metadata.photo_library_taxonomy_pending
            WHERE library_uuid = ?
            "#,
            [&library.library_uuid],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()?;
    let latest_sync_id = pending.map_or(last_sync_id, |pending| pending.0);
    let identity_changed = bound_identity != taxonomy_identity;
    let full_remap = identity_changed || pending.is_some_and(|pending| pending.1);

    if identity_changed {
        transaction.execute("DELETE FROM photo_taxon_candidate_names", [])?;
        transaction.execute("DELETE FROM photo_taxon_candidates", [])?;
        transaction.execute("DELETE FROM photo_taxon_mapping", [])?;
        transaction.execute("DELETE FROM photo_taxon_usage", [])?;
        transaction.execute("DELETE FROM photo_mapping_queue", [])?;
    }

    if full_remap {
        transaction.execute(
            r#"
            INSERT INTO photo_mapping_queue (photo_id, reason)
            SELECT photo_id, 'taxonomy' FROM photos
            WHERE true
            ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason
            "#,
            [],
        )?;
    } else if pending.is_some() {
        transaction.execute_batch(
            r#"
            CREATE TEMP TABLE affected_sync_taxa (
                taxon_id INTEGER PRIMARY KEY
            ) WITHOUT ROWID;
            "#,
        )?;
        transaction.execute(
            r#"
            INSERT INTO affected_sync_taxa (taxon_id)
            SELECT taxon_id
            FROM metadata.photo_library_taxonomy_pending_taxa
            WHERE library_uuid = ?
            "#,
            [&library.library_uuid],
        )?;
        transaction.execute(
            r#"
            INSERT INTO photo_mapping_queue (photo_id, reason)
            SELECT DISTINCT affected.photo_id, 'taxonomy'
            FROM (
                SELECT mapping.photo_id, mapping.taxon_id
                FROM photo_taxon_mapping AS mapping
                WHERE mapping.taxon_id IS NOT NULL
                UNION ALL
                SELECT candidates.photo_id, candidates.taxon_id
                FROM photo_taxon_candidates AS candidates
            ) AS affected
            JOIN affected_sync_taxa USING (taxon_id)
            ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason
            "#,
            [],
        )?;
        transaction.execute_batch(
            r#"
            CREATE TEMP TABLE deleted_sync_taxa (
                taxon_id INTEGER PRIMARY KEY
            ) WITHOUT ROWID;
            INSERT INTO deleted_sync_taxa (taxon_id)
            SELECT affected.taxon_id
            FROM affected_sync_taxa AS affected
            LEFT JOIN taxonomy.taxa AS taxa USING (taxon_id)
            WHERE taxa.taxon_id IS NULL;

            CREATE TEMP TABLE deleted_sync_photos (
                photo_id INTEGER PRIMARY KEY
            ) WITHOUT ROWID;
            INSERT INTO deleted_sync_photos (photo_id)
            SELECT DISTINCT affected.photo_id
            FROM (
                SELECT mapping.photo_id, mapping.taxon_id
                FROM photo_taxon_mapping AS mapping
                WHERE mapping.taxon_id IS NOT NULL
                UNION ALL
                SELECT candidates.photo_id, candidates.taxon_id
                FROM photo_taxon_candidates AS candidates
            ) AS affected
            JOIN deleted_sync_taxa USING (taxon_id);

            DELETE FROM photo_taxon_mapping
            WHERE photo_id IN (SELECT photo_id FROM deleted_sync_photos);

            DELETE FROM photo_taxon_usage;
            INSERT INTO photo_taxon_usage (
                taxon_id, direct_photo_count, subtree_photo_count
            )
            WITH RECURSIVE taxon_paths(direct_taxon_id, taxon_id) AS (
                SELECT mapping.taxon_id, mapping.taxon_id
                FROM photo_taxon_mapping AS mapping
                JOIN taxonomy.taxa AS taxa
                  ON taxa.taxon_id = mapping.taxon_id
                WHERE mapping.status = 'matched'
                UNION ALL
                SELECT paths.direct_taxon_id, taxa.parent_taxon_id
                FROM taxon_paths AS paths
                JOIN taxonomy.taxa AS taxa ON taxa.taxon_id = paths.taxon_id
                WHERE taxa.parent_taxon_id IS NOT NULL
            )
            SELECT taxon_id,
                   SUM(direct_taxon_id = taxon_id),
                   COUNT(*)
            FROM taxon_paths
            GROUP BY taxon_id;
            "#,
        )?;
    }

    transaction.execute(
        r#"
        UPDATE photo_library
        SET bound_taxonomy_identity = ?,
            last_taxonomy_sync_id = ?
        WHERE library_id = 1
        "#,
        params![taxonomy_identity, latest_sync_id],
    )?;
    transaction.execute(
        r#"
        DELETE FROM metadata.photo_library_taxonomy_pending
        WHERE library_uuid = ?
        "#,
        [&library.library_uuid],
    )?;
    let queued_photo_count =
        transaction.query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
            row.get::<_, i64>(0)
        })?;
    transaction.commit()?;
    Ok(TaxonomySyncResult {
        library_uuid: library.library_uuid.clone(),
        sync_id: latest_sync_id,
        queued_photo_count,
        full_remap,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn targeted_events_only_queue_related_photos_in_every_library() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let root_a = directory.path().join("root-a");
        let root_b = directory.path().join("root-b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let library_a = database
            .register_photo_library(&root_a, &directory.path().join("a.db"), Some("A"))
            .unwrap();
        let library_b = database
            .register_photo_library(&root_b, &directory.path().join("b.db"), Some("B"))
            .unwrap();
        database
            .connect_taxonomy()
            .unwrap()
            .execute_batch(
                r#"
                INSERT INTO taxa (taxon_id, rank) VALUES (10, 5), (20, 5);
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (10, 1, 'Alpha beta'), (20, 1, 'Gamma delta');
                "#,
            )
            .unwrap();
        for library in [&library_a, &library_b] {
            let connection = database
                .connect_photo_library_registration(library)
                .unwrap();
            connection
                .execute_batch(
                    r#"
                    INSERT INTO photos (
                        photo_id, directory_id, filename,
                        file_size, modified_at_ns
                    ) SELECT 1, directory_id, 'one.jpg', 1, 1
                      FROM photo_directories WHERE relative_path = '';
                    INSERT INTO photos (
                        photo_id, directory_id, filename,
                        file_size, modified_at_ns
                    ) SELECT 2, directory_id, 'two.jpg', 1, 1
                      FROM photo_directories WHERE relative_path = '';
                    INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                    VALUES (1, 10, 'matched'), (2, 20, 'matched');
                    "#,
                )
                .unwrap();
        }
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy.transaction().unwrap();
        record_event(&transaction, None, [10], false).unwrap();
        transaction.commit().unwrap();
        synchronize_pending_photo_libraries(&database).unwrap();
        for library in [&library_a, &library_b] {
            let connection = database
                .connect_photo_library_registration(library)
                .unwrap();
            let queued = connection
                .prepare("SELECT photo_id FROM photo_mapping_queue ORDER BY photo_id")
                .unwrap()
                .query_map([], |row| row.get::<_, i64>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(queued, [1]);
        }
    }

    #[test]
    fn identity_changes_clear_mappings_and_queue_every_photo() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let root = directory.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let library = database
            .register_photo_library(&root, &directory.path().join("library.db"), Some("Library"))
            .unwrap();
        database
            .connect_taxonomy()
            .unwrap()
            .execute("INSERT INTO taxa (taxon_id, rank) VALUES (10, 5)", [])
            .unwrap();
        let connection = database
            .connect_photo_library_registration(&library)
            .unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO photos (
                    photo_id, directory_id, filename,
                    file_size, modified_at_ns
                ) SELECT 1, directory_id, 'one.jpg', 1, 1
                  FROM photo_directories WHERE relative_path = '';
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (1, 10, 'matched');
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (10, 1, 1);
                "#,
            )
            .unwrap();
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy.transaction().unwrap();
        transaction
            .execute(
                "UPDATE taxonomy_identity SET taxonomy_identity = ? WHERE identity_id = 1",
                [uuid::Uuid::new_v4().to_string()],
            )
            .unwrap();
        record_event(&transaction, None, [], true).unwrap();
        transaction.commit().unwrap();

        let results = synchronize_pending_photo_libraries(&database).unwrap();
        assert!(results.synchronized.iter().any(|result| result.full_remap));
        let connection = database
            .connect_photo_library_registration(&library)
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM photo_taxon_mapping", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn unavailable_libraries_keep_pending_without_failing_other_libraries() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("metadata.db")).unwrap();
        let root_a = directory.path().join("root-a");
        let root_b = directory.path().join("root-b");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();
        let library_a = database
            .register_photo_library(&root_a, &directory.path().join("a.db"), Some("A"))
            .unwrap();
        let library_b = database
            .register_photo_library(&root_b, &directory.path().join("b.db"), Some("B"))
            .unwrap();
        database
            .switch_photo_library(&library_a.library_uuid)
            .unwrap();
        fs::remove_file(&library_b.db_path).unwrap();
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy.transaction().unwrap();
        record_event(&transaction, None, [10, 10, 20], false).unwrap();
        record_event(&transaction, None, [20, 30], false).unwrap();
        transaction.commit().unwrap();

        let run = synchronize_pending_photo_libraries(&database).unwrap();

        assert_eq!(run.synchronized.len(), 1);
        assert_eq!(run.synchronized[0].library_uuid, library_a.library_uuid);
        assert_eq!(
            run.pending_library_uuids.as_slice(),
            std::slice::from_ref(&library_b.library_uuid)
        );
        let metadata = database.connect_metadata().unwrap();
        assert_eq!(
            metadata
                .query_row(
                    r#"
                    SELECT COUNT(*)
                    FROM photo_library_taxonomy_pending_taxa
                    WHERE library_uuid = ?
                    "#,
                    [&library_b.library_uuid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            3
        );
        assert_eq!(
            metadata
                .query_row(
                    r#"
                    SELECT COUNT(*)
                    FROM photo_library_taxonomy_pending
                    WHERE library_uuid = ?
                    "#,
                    [&library_a.library_uuid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }
}
