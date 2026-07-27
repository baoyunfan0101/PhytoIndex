use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, Transaction, TransactionBehavior, params};

use crate::models::PhotoLibraryRegistration;
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaxonomySyncResult {
    pub sync_id: i64,
    pub queued_photo_count: i64,
    pub full_remap: bool,
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

pub(crate) fn synchronize_all_photo_libraries(
    database: &Database,
) -> CoreResult<Vec<TaxonomySyncResult>> {
    let libraries = database.list_photo_libraries()?;
    let mut results = Vec::with_capacity(libraries.len());
    for library in libraries {
        results.push(synchronize_photo_library(database, &library)?);
    }
    cleanup_consumed_events(database)?;
    Ok(results)
}

pub(crate) fn synchronize_photo_library(
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
    let latest_sync_id = transaction.query_row(
        "SELECT COALESCE(MAX(sync_id), 0) FROM taxonomy.taxonomy_sync_events",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    let identity_changed = bound_identity != taxonomy_identity;
    let oldest_retained_sync_id = transaction.query_row(
        "SELECT MIN(sync_id) FROM taxonomy.taxonomy_sync_events",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    let fell_behind =
        oldest_retained_sync_id.is_some_and(|oldest| last_sync_id.saturating_add(1) < oldest);
    let event_requires_full_remap = transaction.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM taxonomy.taxonomy_sync_events
            WHERE sync_id > ? AND full_remap_required = 1
        )
        "#,
        [last_sync_id],
        |row| row.get::<_, bool>(0),
    )?;
    let full_remap = identity_changed || fell_behind || event_requires_full_remap;

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
    } else if latest_sync_id > last_sync_id {
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
            SELECT DISTINCT event_taxa.taxon_id
            FROM taxonomy.taxonomy_sync_event_taxa AS event_taxa
            WHERE event_taxa.sync_id > ?
            "#,
            [last_sync_id],
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
    let queued_photo_count =
        transaction.query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
            row.get::<_, i64>(0)
        })?;
    transaction.commit()?;
    Ok(TaxonomySyncResult {
        sync_id: latest_sync_id,
        queued_photo_count,
        full_remap,
    })
}

pub(crate) fn cleanup_consumed_events(database: &Database) -> CoreResult<()> {
    let taxonomy_identity = database.taxonomy_identity()?;
    let libraries = database.list_photo_libraries()?;
    let mut consumed_through = None::<i64>;
    for library in libraries {
        let connection = match database.connect_photo_library_registration(&library) {
            Ok(connection) => connection,
            Err(_) => return Ok(()),
        };
        let state = connection
            .query_row(
                r#"
                SELECT bound_taxonomy_identity, last_taxonomy_sync_id
                FROM photo_library
                WHERE library_id = 1
                "#,
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((bound_identity, last_sync_id)) = state else {
            return Err(CoreError::Consistency(format!(
                "photo library {} has no identity row",
                library.library_uuid
            )));
        };
        if bound_identity != taxonomy_identity {
            return Ok(());
        }
        consumed_through =
            Some(consumed_through.map_or(last_sync_id, |current| current.min(last_sync_id)));
    }
    if let Some(consumed_through) = consumed_through {
        database.connect_taxonomy()?.execute(
            "DELETE FROM taxonomy_sync_events WHERE sync_id <= ?",
            [consumed_through],
        )?;
    }
    Ok(())
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
        synchronize_all_photo_libraries(&database).unwrap();
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

        let results = synchronize_all_photo_libraries(&database).unwrap();
        assert!(results.iter().any(|result| result.full_remap));
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
}
