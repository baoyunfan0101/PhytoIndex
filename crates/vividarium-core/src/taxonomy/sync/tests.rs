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
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM taxonomy_sync_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}

#[test]
fn dispatched_events_are_deleted_without_reusing_sync_ids() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let mut taxonomy = database.connect_taxonomy().unwrap();
    let transaction = taxonomy.transaction().unwrap();
    let first_sync_id = record_event(&transaction, None, [10], false).unwrap();
    transaction.commit().unwrap();
    synchronize_pending_photo_libraries(&database).unwrap();
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM taxonomy_sync_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );

    let mut taxonomy = database.connect_taxonomy().unwrap();
    let transaction = taxonomy.transaction().unwrap();
    let second_sync_id = record_event(&transaction, None, [20], false).unwrap();
    transaction.commit().unwrap();
    assert!(second_sync_id > first_sync_id);
    synchronize_pending_photo_libraries(&database).unwrap();
    assert_eq!(database.latest_taxonomy_sync_id().unwrap(), second_sync_id);
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM taxonomy_sync_events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}
