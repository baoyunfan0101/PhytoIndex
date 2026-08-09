use super::*;

#[test]
fn initializes_metadata_and_taxonomy_without_a_fake_photo_library() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let locations = database.locations().unwrap();
    assert_eq!(
        locations.metadata_database,
        path_string(&database.metadata_path())
    );
    assert_eq!(
        schema_version(&database.connect_metadata().unwrap()),
        SCHEMA_VERSION
    );
    assert_eq!(
        schema_version(&database.connect_taxonomy().unwrap()),
        SCHEMA_VERSION
    );
    assert_ne!(locations.metadata_database, locations.taxonomy_database);
    assert_eq!(database.active_photo_library().unwrap(), None);
    assert!(database.list_photo_libraries().unwrap().is_empty());
    assert_eq!(locations.active_photo_library_uuid, None);
}

#[test]
fn opens_an_existing_taxonomy_database_and_marks_libraries_for_remap() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open_test(directory.path().join("metadata.db")).unwrap();
    database
        .connect_metadata()
        .unwrap()
        .execute(
            "UPDATE taxonomy_sync_dispatch SET last_dispatched_sync_id = 42 WHERE dispatch_id = 1",
            [],
        )
        .unwrap();
    let taxonomy_path = directory.path().join("alternate-taxonomy.db");
    initialize_taxonomy_database_file(&taxonomy_path).unwrap();
    let expected_identity = open_existing_connection(&taxonomy_path)
        .unwrap()
        .query_row(
            "SELECT taxonomy_identity FROM taxonomy_identity WHERE identity_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();

    let locations = database.open_taxonomy_database(&taxonomy_path).unwrap();

    assert_eq!(
        locations.taxonomy_database,
        path_string(&fs::canonicalize(&taxonomy_path).unwrap())
    );
    assert_eq!(database.taxonomy_identity().unwrap(), expected_identity);
    let metadata = database.connect_metadata().unwrap();
    assert_eq!(
        metadata
            .query_row(
                "SELECT last_dispatched_sync_id FROM taxonomy_sync_dispatch WHERE dispatch_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        metadata
            .query_row(
                "SELECT COUNT(*) FROM photo_library_taxonomy_pending WHERE full_remap_required = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn rejects_a_non_taxonomy_database_when_opening_taxonomy_storage() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let photo_database = directory.path().join("photo.db");
    initialize_file(&photo_database, PHOTO_SCHEMA).unwrap();

    let error = database
        .open_taxonomy_database(&photo_database)
        .unwrap_err();

    assert!(matches!(error, CoreError::InvalidArgument(_)));
    assert_ne!(
        database.taxonomy_path().unwrap(),
        fs::canonicalize(photo_database).unwrap()
    );
}

#[test]
fn opening_taxonomy_storage_does_not_initialize_an_empty_database() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let empty_database = directory.path().join("empty.db");
    Connection::open(&empty_database).unwrap();

    let error = database
        .open_taxonomy_database(&empty_database)
        .unwrap_err();

    assert!(matches!(error, CoreError::InvalidArgument(_)));
    assert_eq!(
        open_existing_connection(&empty_database)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn switches_between_independent_photo_libraries() {
    let directory = tempfile::tempdir().unwrap();
    let root_a = directory.path().join("photos-a");
    let root_b = directory.path().join("photos-b");
    fs::create_dir_all(&root_a).unwrap();
    fs::create_dir_all(&root_b).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let library_a = database
        .register_photo_library(&root_a, &directory.path().join("a.db"), Some("A"))
        .unwrap();
    database
        .connect()
        .unwrap()
        .execute(
            r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                ) SELECT directory_id, 'a.jpg', 1, 1
                  FROM photo_directories WHERE relative_path = ''
                "#,
            [],
        )
        .unwrap();
    let library_b = database
        .register_photo_library(&root_b, &directory.path().join("b.db"), Some("B"))
        .unwrap();
    assert_eq!(
        database
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    database
        .switch_photo_library(&library_a.library_uuid)
        .unwrap();
    assert_eq!(
        database
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_ne!(library_a.library_uuid, library_b.library_uuid);
}

#[test]
fn switching_a_missing_registered_database_does_not_recreate_it() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let database_path = directory.path().join("library.db");
    let library = database
        .register_photo_library(&root, &database_path, Some("Library"))
        .unwrap();
    fs::remove_file(&database_path).unwrap();

    let error = database
        .switch_photo_library(&library.library_uuid)
        .unwrap_err();

    assert!(matches!(error, CoreError::NotFound(_)));
    assert!(!database_path.exists());
}

#[test]
fn ordinary_connections_do_not_recreate_a_missing_active_library() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let database_path = directory.path().join("library.db");
    database
        .register_photo_library(&root, &database_path, Some("Library"))
        .unwrap();
    fs::remove_file(&database_path).unwrap();

    assert!(matches!(database.connect(), Err(CoreError::NotFound(_))));
    assert!(!database_path.exists());
    database.connect_taxonomy_metadata_context().unwrap();
    assert!(matches!(
        database.connect_taxonomy_photo_context(),
        Err(CoreError::NotFound(_))
    ));
    assert!(!database_path.exists());
    assert!(
        crate::taxonomy::suggest_taxa(&database, "missing", 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        crate::taxonomy::get_taxonomy_name_separator(&database).unwrap(),
        ";"
    );
}

#[test]
fn rebinds_a_missing_active_library_database_and_restores_sync() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let old_path = directory.path().join("old.db");
    let new_path = directory.path().join("new.db");
    let library = database
        .register_photo_library(&root, &old_path, Some("Library"))
        .unwrap();
    database
        .connect()
        .unwrap()
        .execute(
            r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                )
                SELECT directory_id, 'photo.jpg', 1, 1
                FROM photo_directories
                WHERE relative_path = ''
                "#,
            [],
        )
        .unwrap();
    fs::rename(&old_path, &new_path).unwrap();
    assert!(matches!(database.connect(), Err(CoreError::NotFound(_))));

    {
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        crate::taxonomy::sync::record_event(&transaction, None, [42], false).unwrap();
        transaction.commit().unwrap();
    }
    crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
    let latest_sync_id = database.latest_taxonomy_sync_id().unwrap();
    database
        .connect_metadata()
        .unwrap()
        .execute(
            "DELETE FROM photo_library_taxonomy_pending WHERE library_uuid = ?",
            [&library.library_uuid],
        )
        .unwrap();

    let rebound = database
        .rebind_photo_library_database(&library.library_uuid, &new_path)
        .unwrap();

    assert_eq!(rebound.db_path, path_string(&new_path));
    assert_eq!(
        database
            .connect_metadata()
            .unwrap()
            .query_row(
                r#"
                    SELECT target_sync_id, full_remap_required
                    FROM photo_library_taxonomy_pending
                    WHERE library_uuid = ?
                    "#,
                [&library.library_uuid],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, bool>(1)?)),
            )
            .unwrap(),
        (latest_sync_id, true)
    );
    crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
    assert_eq!(
        database
            .connect()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
}

#[test]
fn rejects_rebinding_to_a_different_library_database() {
    let directory = tempfile::tempdir().unwrap();
    let root_a = directory.path().join("photos-a");
    let root_b = directory.path().join("photos-b");
    fs::create_dir_all(&root_a).unwrap();
    fs::create_dir_all(&root_b).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let path_a = directory.path().join("a.db");
    let path_b = directory.path().join("b.db");
    let library_a = database
        .register_photo_library(&root_a, &path_a, Some("A"))
        .unwrap();
    database
        .register_photo_library(&root_b, &path_b, Some("B"))
        .unwrap();

    let error = database
        .rebind_photo_library_database(&library_a.library_uuid, &path_b)
        .unwrap_err();

    assert!(matches!(error, CoreError::InvalidArgument(_)));
    assert_eq!(
        database
            .photo_library(&library_a.library_uuid)
            .unwrap()
            .db_path,
        path_string(&path_a)
    );
}

#[test]
fn photo_library_roots_are_unique() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    database
        .register_photo_library(&root, &directory.path().join("first.db"), Some("First"))
        .unwrap();

    let error = database
        .register_photo_library(&root, &directory.path().join("second.db"), Some("Second"))
        .unwrap_err();

    assert!(matches!(error, CoreError::InvalidArgument(_)));
    assert_eq!(database.list_photo_libraries().unwrap().len(), 1);
    assert!(!directory.path().join("second.db").exists());
}

#[test]
fn renames_a_photo_library_registration() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let library = database
        .register_photo_library(&root, &directory.path().join("library.db"), Some("Before"))
        .unwrap();

    let renamed = database
        .rename_photo_library(&library.library_uuid, "  After  ")
        .unwrap();

    assert_eq!(renamed.display_name, "After");
    assert_eq!(
        database.list_photo_libraries().unwrap()[0].display_name,
        "After"
    );
    assert!(matches!(
        database.rename_photo_library(&library.library_uuid, "  "),
        Err(CoreError::InvalidArgument(_))
    ));
}

#[test]
fn registering_a_stale_library_queues_a_full_remap() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let taxonomy_identity = database.taxonomy_identity().unwrap();
    {
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        crate::taxonomy::sync::record_event(&transaction, None, [42], false).unwrap();
        transaction.commit().unwrap();
    }
    crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
    let latest_sync_id = database.latest_taxonomy_sync_id().unwrap();
    assert!(latest_sync_id > 0);
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

    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let library_path = directory.path().join("stale.db");
    initialize_file(&library_path, PHOTO_SCHEMA).unwrap();
    let library_uuid = new_uuid();
    {
        let connection = open_existing_connection(&library_path).unwrap();
        connection
            .execute(
                r#"
                    INSERT INTO photo_library (
                        library_id, library_uuid, root_path,
                        bound_taxonomy_identity, last_taxonomy_sync_id
                    ) VALUES (1, ?, ?, ?, 0)
                    "#,
                params![library_uuid, path_string(&root), taxonomy_identity],
            )
            .unwrap();
        ensure_photo_root(&connection).unwrap();
        connection
            .execute(
                r#"
                    INSERT INTO photos (
                        directory_id, filename, file_size, modified_at_ns
                    )
                    SELECT directory_id, 'stale.jpg', 1, 1
                    FROM photo_directories
                    WHERE relative_path = ''
                    "#,
                [],
            )
            .unwrap();
    }

    let registered = database
        .register_photo_library(&root, &library_path, Some("Stale"))
        .unwrap();

    assert_eq!(registered.library_uuid, library_uuid);
    let connection = database.connect().unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT last_taxonomy_sync_id FROM photo_library WHERE library_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        latest_sync_id
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
fn relocation_uses_a_consistent_wal_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("photos");
    fs::create_dir_all(&root).unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let source = directory.path().join("source.db");
    let library = database
        .register_photo_library(&root, &source, Some("Library"))
        .unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                )
                SELECT directory_id, 'wal-photo.jpg', 1, 1
                FROM photo_directories
                WHERE relative_path = ''
                "#,
            [],
        )
        .unwrap();
    let destination = directory.path().join("destination.db");

    database
        .relocate_photo_library_database(&library.library_uuid, &destination)
        .unwrap();
    drop(connection);

    assert!(!source.exists());
    assert!(destination.exists());
    assert_eq!(
        database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM photos WHERE filename = 'wal-photo.jpg'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn rejects_any_other_schema_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("metadata.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA user_version = 1;")
        .unwrap();
    drop(connection);
    let error = Database::open(path).unwrap_err();
    assert!(error.to_string().contains("expected 2"));
}

fn schema_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}
