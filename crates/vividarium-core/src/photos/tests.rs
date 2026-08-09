use super::*;

#[test]
fn opens_and_refreshes_the_requested_directory_subtree() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::write(root.path().join("first.jpg"), b"first").unwrap();
    fs::write(root.path().join("nested").join("second.jpg"), b"second").unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    let result = refresh_directory(&database, library.root_directory_id).unwrap();
    assert_eq!(result.inserted, 2);
    assert_eq!(result.directories_inserted, 1);
    assert_eq!(list_photos(&database).unwrap().len(), 2);
    let listing = browse_directory(&database, library.root_directory_id, None, 20).unwrap();
    assert_eq!(listing.items.len(), 2);
    match &listing.items[0] {
        PhotoDirectoryItem::Directory { .. } => {}
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
    assert_eq!(get_photo_count(&database).unwrap(), 2);
}

#[test]
fn refresh_removes_missing_directory_subtrees() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::create_dir(root.path().join("nested").join("deep")).unwrap();
    fs::write(root.path().join("root.jpg"), b"root").unwrap();
    fs::write(root.path().join("nested").join("nested.jpg"), b"nested").unwrap();
    fs::write(
        root.path().join("nested").join("deep").join("deep.jpg"),
        b"deep",
    )
    .unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    let initial = refresh_directory(&database, library.root_directory_id).unwrap();
    assert_eq!(initial.inserted, 3);
    assert_eq!(initial.directories_inserted, 2);

    fs::remove_dir_all(root.path().join("nested")).unwrap();
    let result = refresh_directory(&database, library.root_directory_id).unwrap();

    assert_eq!(result.unchanged, 1);
    assert_eq!(result.deleted, 2);
    assert_eq!(result.directories_deleted, 2);
    assert_eq!(list_photos(&database).unwrap().len(), 1);
    assert_eq!(get_photo_count(&database).unwrap(), 1);
}

#[test]
fn resolves_photo_directory_path_inside_the_library_root() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    refresh_directory(&database, library.root_directory_id).unwrap();
    let listing = browse_directory(&database, library.root_directory_id, None, 20).unwrap();
    let nested_directory_id = match &listing.items[0] {
        PhotoDirectoryItem::Directory { directory } => directory.directory_id,
        PhotoDirectoryItem::Photo { .. } => panic!("expected a directory first"),
    };

    assert_eq!(
        photo_directory_path(&database, nested_directory_id).unwrap(),
        root.path().join("nested").canonicalize().unwrap(),
    );
}

#[test]
fn renames_photo_directory_and_updates_descendant_paths() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    fs::create_dir(root.path().join("nested").join("deep")).unwrap();
    fs::write(root.path().join("nested").join("nested.jpg"), b"nested").unwrap();
    fs::write(
        root.path().join("nested").join("deep").join("deep.jpg"),
        b"deep",
    )
    .unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    refresh_directory(&database, library.root_directory_id).unwrap();
    let listing = browse_directory(&database, library.root_directory_id, None, 20).unwrap();
    let nested_directory_id = match &listing.items[0] {
        PhotoDirectoryItem::Directory { directory } => directory.directory_id,
        PhotoDirectoryItem::Photo { .. } => panic!("expected a directory first"),
    };

    let renamed = rename_directory(&database, nested_directory_id, "renamed").unwrap();

    assert_eq!(renamed.name, "renamed");
    assert_eq!(renamed.relative_path, "renamed");
    assert!(!root.path().join("nested").exists());
    assert!(root.path().join("renamed").is_dir());
    assert!(
        root.path()
            .join("renamed")
            .join("deep")
            .join("deep.jpg")
            .is_file()
    );
    let photos = list_photos(&database).unwrap();
    assert_eq!(photos.len(), 2);
    assert!(
        photos
            .iter()
            .any(|photo| photo.relative_path == "renamed/nested.jpg")
    );
    assert!(
        photos
            .iter()
            .any(|photo| photo.relative_path == "renamed/deep/deep.jpg")
    );
}

#[test]
fn rejects_renaming_photo_library_root() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();

    let error = rename_directory(&database, library.root_directory_id, "renamed").unwrap_err();

    assert!(error.to_string().contains("root cannot be renamed"));
}

#[test]
fn refresh_queues_a_photo_without_mapping_state() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("photo.jpg"), b"photo").unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    refresh_directory(&database, library.root_directory_id).unwrap();
    let photo = list_photos(&database).unwrap().remove(0);
    database
        .connect()
        .unwrap()
        .execute(
            "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
            [photo.photo_id],
        )
        .unwrap();

    let result = refresh_directory(&database, library.root_directory_id).unwrap();

    assert_eq!(result.unchanged, 1);
    assert_eq!(
        mapping::get_photo_mapping(&database, photo.photo_id)
            .unwrap()
            .status,
        mapping::PhotoTaxonStatus::Processing
    );
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

    let operations = list_operations(&database, None, 10).unwrap().items;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].source, "manual_rename");
    assert_eq!(operations[0].total_items, 1);
    assert_eq!(operations[0].succeeded_items, 1);
    let audit = list_operation_audit(&database, operations[0].operation_id, None, 10).unwrap();
    assert_eq!(audit.items.len(), 1);
    let photo_id = photo.photo_id.to_string();
    assert_eq!(audit.items[0].entity_id.as_deref(), Some(photo_id.as_str()));
    assert_eq!(audit.items[0].action, "rename");

    rollback_operation(&database, operations[0].operation_id).unwrap();

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
        list_operations(&database, None, 10)
            .unwrap()
            .items
            .is_empty()
    );
    let error = rollback_operation(&database, operations[0].operation_id).unwrap_err();
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
    let first_operation_id = list_operations(&database, None, 10).unwrap().items[0].operation_id;
    rename_photo(&database, photo.photo_id, "third.jpg").unwrap();
    let second_operation_id = list_operations(&database, None, 10).unwrap().items[0].operation_id;
    let newest_operation = list_operations(&database, None, 1).unwrap();
    assert_eq!(newest_operation.items[0].operation_id, second_operation_id);
    assert!(newest_operation.next_cursor.is_some());
    let older_operation =
        list_operations(&database, newest_operation.next_cursor.as_deref(), 1).unwrap();
    assert_eq!(older_operation.items[0].operation_id, first_operation_id);
    assert_eq!(older_operation.next_cursor, None);
    let error = rollback_operation(&database, first_operation_id).unwrap_err();
    assert!(error.to_string().contains("expected 'second.jpg'"));
    assert!(root.path().join("third.jpg").is_file());

    rollback_operation(&database, second_operation_id).unwrap();
    assert!(root.path().join("second.jpg").is_file());
    rollback_operation(&database, first_operation_id).unwrap();
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
                VALUES (?, 1, 'Canis lupus')
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
    let operation = list_operations(&database, None, 10)
        .unwrap()
        .items
        .remove(0);
    assert_eq!(renamed.operation_id, Some(operation.operation_id));
    assert_eq!(operation.source, "taxon_selection_rename");
    assert_eq!(operation.succeeded_items, 2);
    let audit = list_operation_audit(&database, operation.operation_id, None, 10).unwrap();
    assert_eq!(audit.items.len(), 2);
    assert_eq!(audit.items[0].sequence, 1);
    assert_eq!(audit.items[1].sequence, 2);
    crate::general::update_general_settings(
        &database,
        &crate::general::GeneralSettings {
            csv_delimiter: "\t".into(),
            ..crate::general::GeneralSettings::default()
        },
    )
    .unwrap();
    let mut output = Vec::new();
    write_operation_audit(&database, operation.operation_id, &mut output).unwrap();
    let exported = String::from_utf8(output).unwrap();
    assert_eq!(exported.lines().count(), 3);
    assert!(exported.starts_with("operation_id\tsequence\t"));

    rollback_operation(&database, operation.operation_id).unwrap();
    assert!(root.path().join("first.jpg").is_file());
    assert!(root.path().join("second.png").is_file());
}

#[test]
fn directory_taxon_rename_selects_only_current_mappings() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("mapped.jpg"), b"mapped").unwrap();
    fs::write(root.path().join("processing.png"), b"processing").unwrap();
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
                VALUES (?, 1, 'Canis lupus')
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
        rename_photos_in_directory_from_taxa(&database, library.root_directory_id, false).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].photo_id, mapped_photo.photo_id);
    assert_eq!(result.rows[0].status, PhotoRenameRowStatus::Applied);
    assert!(root.path().join("Canis lupus.jpg").is_file());
    assert!(root.path().join("processing.png").is_file());
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
                VALUES (?, 1, 'Canis lupus')
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
    assert_eq!(result.rows[1].operation_id, result.operation_id);
    assert!(
        result.rows[1]
            .message
            .contains("rename destination already exists")
    );
    assert_eq!(result.rows[2].status, PhotoRenameRowStatus::Applied);
    assert!(result.rows[2].operation_id.is_some());
    let operation =
        list_operation_audit(&database, result.operation_id.unwrap(), None, 10).unwrap();
    assert_eq!(
        operation
            .items
            .iter()
            .map(|item| item.sequence)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert!(!operation.items[1].succeeded);
    assert!(root.path().join("Canis lupus.jpg").is_file());
    assert!(root.path().join("second.jpg").is_file());
    assert!(root.path().join("Canis lupus.png").is_file());
}

#[test]
fn records_an_operation_when_every_rename_row_fails() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("unmapped.jpg"), b"photo").unwrap();
    let database = Database::open(data.path().join("vividarium.db")).unwrap();
    let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
    refresh_directory(&database, library.root_directory_id).unwrap();
    let photo = list_photos(&database).unwrap().remove(0);

    let result = rename_photos_from_taxa(&database, &[photo.photo_id]).unwrap();

    let operation_id = result.operation_id.unwrap();
    assert_eq!(result.rows[0].status, PhotoRenameRowStatus::Failed);
    assert_eq!(result.rows[0].operation_id, Some(operation_id));
    let summary = list_operations(&database, None, 1).unwrap().items.remove(0);
    assert_eq!(summary.operation_id, operation_id);
    assert_eq!(summary.total_items, 1);
    assert_eq!(summary.succeeded_items, 0);
    assert_eq!(summary.failed_items, 1);
    let audit = list_operation_audit(&database, operation_id, None, 10).unwrap();
    assert_eq!(audit.items.len(), 1);
    assert!(!audit.items[0].succeeded);
}

#[test]
fn renames_a_matched_photo_with_its_accepted_scientific_name() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("canis lupus.JPG"), b"photo").unwrap();
    let database = Database::open_test(data.path().join("vividarium.db")).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("INSERT INTO taxa (rank) VALUES (5)", [])
        .unwrap();
    let taxon_id = connection.last_insert_rowid();
    connection
        .execute(
            r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Canis lupus')
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
    mapping::set_photo_mapping(&database, photo.photo_id, taxon_id).unwrap();
    let mut taxonomy = database.connect_taxonomy().unwrap();
    let transaction = taxonomy.transaction().unwrap();
    crate::taxonomy::sync::record_event(&transaction, None, [taxon_id], false).unwrap();
    transaction.commit().unwrap();
    crate::taxonomy::sync::synchronize_pending_photo_libraries(&database).unwrap();
    let error = rename_photo_from_taxon(&database, photo.photo_id).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("must have a current matched taxon")
    );
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
    let operation_id = list_operations(&database, None, 1).unwrap().items[0].operation_id;
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

    let error = rollback_operation(&database, operation_id).unwrap_err();

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
