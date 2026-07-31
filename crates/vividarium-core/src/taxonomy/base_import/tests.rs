use super::*;
use crate::taxonomy::{TaxonInputRow, apply_rows, get_taxon_detail, list_operations};

const SIMPLE_IMPORT_SQL: &str = r#"
ATTACH DATABASE 'vividarium_base.db' AS base;
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;
CREATE TABLE base.taxa (
    taxon_id INTEGER PRIMARY KEY,
    parent_taxon_id INTEGER,
    rank INTEGER NOT NULL,
    geological_range TEXT,
    CHECK (rank BETWEEN 1 AND 5),
    FOREIGN KEY (parent_taxon_id)
        REFERENCES taxa(taxon_id) ON DELETE RESTRICT
);
CREATE TABLE base.taxon_names (
    name_id INTEGER PRIMARY KEY,
    taxon_id INTEGER NOT NULL,
    name_type INTEGER NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT,
    authority_year TEXT,
    source TEXT,
    UNIQUE (taxon_id, name_type, name),
    CHECK (name_type BETWEEN 1 AND 6),
    CHECK (length(trim(name)) > 0),
    FOREIGN KEY (taxon_id)
        REFERENCES taxa(taxon_id) ON DELETE CASCADE
);
INSERT INTO base.taxa
SELECT CAST(taxon_id AS INTEGER), NULL, CAST(rank AS INTEGER), geological_range
FROM main.source_taxa;
INSERT INTO base.taxon_names
SELECT 1, CAST(taxon_id AS INTEGER), 1, name, NULL, NULL, 'test'
FROM main.source_taxa;
COMMIT;
DETACH DATABASE base;
"#;

fn simple_session(directory: &tempfile::TempDir, database: &Database) -> BaseImportSession {
    let csv_path = directory.path().join("simple-source.csv");
    fs::write(
        &csv_path,
        "taxon_id,rank,name,geological_range\n101,1,Animalia,Recent\n",
    )
    .unwrap();
    let session = create_base_import_session(database).unwrap();
    add_base_import_csv_source(
        database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();
    execute_base_import_sql(
        database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();
    session
}

#[test]
fn imports_csv_through_staging_and_atomically_applies_it() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Old kingdom".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let old_identity = database.taxonomy_identity().unwrap();
    let photo_root_a = directory.path().join("photos-a");
    let photo_root_b = directory.path().join("photos-b");
    fs::create_dir_all(&photo_root_a).unwrap();
    fs::create_dir_all(&photo_root_b).unwrap();
    database
        .register_photo_library(
            &photo_root_a,
            &directory.path().join("photos-a.db"),
            Some("Photos A"),
        )
        .unwrap();
    database
        .register_photo_library(
            &photo_root_b,
            &directory.path().join("photos-b.db"),
            Some("Photos B"),
        )
        .unwrap();
    let csv_path = directory.path().join("taxa.csv");
    fs::write(
        &csv_path,
        "taxon_id,rank,name,geological_range\n101,1, Animalia ,Recent\n",
    )
    .unwrap();
    let session = create_base_import_session(&database).unwrap();
    add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();
    let execution = execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();
    assert!(execution.statements_executed > 0);
    let validation = validate_base_import(&database, &session.session_id).unwrap();
    assert!(validation.can_apply, "{:?}", validation.errors);
    assert_eq!(validation.normalization_changes, 1);

    let result = apply_base_import(&database, &session.session_id).unwrap();

    assert_eq!(result.metadata.taxa_count, 1);
    assert_ne!(database.taxonomy_identity().unwrap(), old_identity);
    assert_eq!(
        get_taxon_detail(&database, 101)
            .unwrap()
            .unwrap()
            .names
            .sci_name
            .unwrap()
            .name,
        "Animalia"
    );
    assert!(
        list_operations(&database, None, 10)
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        database
            .connect_metadata()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM photo_library_taxonomy_pending WHERE full_remap_required = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
    discard_base_import_session(&database, &session.session_id).unwrap();
}

#[test]
fn validation_blocks_invalid_staging_and_preserves_taxonomy() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Existing".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let identity = database.taxonomy_identity().unwrap();
    let session = create_base_import_session(&database).unwrap();
    let invalid_sql = SIMPLE_IMPORT_SQL
        .replace(
            "FROM main.source_taxa",
            "FROM (SELECT 101 AS taxon_id, 1 AS rank, 'Invalid' AS name, NULL AS geological_range)",
        )
        .replace("COMMIT;", "DELETE FROM base.taxon_names;\nCOMMIT;");
    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: invalid_sql,
        },
    )
    .unwrap();

    let validation = validate_base_import(&database, &session.session_id).unwrap();

    assert!(!validation.can_apply);
    assert!(apply_base_import(&database, &session.session_id).is_err());
    assert_eq!(database.taxonomy_identity().unwrap(), identity);
}

#[test]
fn base_import_rejects_any_other_attachment() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = create_base_import_session(&database).unwrap();
    let error = execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id,
            sql: "ATTACH DATABASE 'other.db' AS base".into(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("not authorized"));
    assert!(!directory.path().join("other.db").exists());
}

#[test]
fn base_import_requires_the_base_attachment_alias() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = create_base_import_session(&database).unwrap();
    let error = execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id,
            sql: "ATTACH DATABASE 'vividarium_base.db' AS staging".into(),
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("staging database must use the base alias")
    );
}

#[test]
fn built_in_sql_imports_a_sqlite_source() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let source_path = directory.path().join("source.db");
    let source = Connection::open(&source_path).unwrap();
    source
        .execute_batch(
            r#"
                CREATE TABLE taxa (
                    id INTEGER PRIMARY KEY,
                    parent INTEGER,
                    category INTEGER,
                    rank INTEGER,
                    scientific_name TEXT,
                    authority_year TEXT,
                    geological_range TEXT,
                    english_name TEXT
                );
                CREATE TABLE synonyms (
                    parent INTEGER,
                    category INTEGER,
                    synonym TEXT,
                    authority_year TEXT,
                    PRIMARY KEY (parent, synonym, authority_year)
                );
                CREATE TABLE chinese (
                    id INTEGER,
                    is_accepted INTEGER,
                    chinese_name TEXT,
                    source TEXT,
                    PRIMARY KEY (id, chinese_name)
                );
                INSERT INTO taxa VALUES
                    (10, NULL, 0, 60, 'Animalia', NULL, 'Recent', 'Animals');
                INSERT INTO synonyms VALUES (10, 0, 'Metazoa', NULL);
                INSERT INTO chinese VALUES (10, 1, 'Dong wu jie', 'test');
                "#,
        )
        .unwrap();
    drop(source);
    let session = create_base_import_session(&database).unwrap();
    let schema = add_base_import_sqlite_source(
        &database,
        &AddBaseImportSqliteSourceRequest {
            session_id: session.session_id.clone(),
            path: source_path,
        },
    )
    .unwrap();
    assert!(schema.objects.iter().any(|object| object.name == "taxa"));

    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: get_default_base_import_sql(&database).unwrap(),
        },
    )
    .unwrap();
    let validation = validate_base_import(&database, &session.session_id).unwrap();

    assert!(validation.can_apply, "{:?}", validation.errors);
    assert_eq!(validation.taxa_count, 1);
    assert_eq!(
        validation
            .name_counts
            .iter()
            .map(|count| count.count)
            .sum::<u64>(),
        4
    );
}

#[test]
fn validate_reuses_candidate_and_apply_uses_its_identity() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = simple_session(&directory, &database);

    let first = validate_base_import(&database, &session.session_id).unwrap();
    assert!(first.can_apply);
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    let state = read_session_state(&workspace).unwrap();
    let candidate = state.candidate.unwrap();
    let candidate_identity = Connection::open(&candidate.candidate_path)
        .unwrap()
        .query_row(
            "SELECT taxonomy_identity FROM taxonomy_identity WHERE identity_id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();

    let second = validate_base_import(&database, &session.session_id).unwrap();
    assert_eq!(second, first);
    assert_eq!(
        read_session_state(&workspace)
            .unwrap()
            .candidate
            .unwrap()
            .staging_fingerprint,
        candidate.staging_fingerprint
    );

    apply_base_import(&database, &session.session_id).unwrap();

    assert_eq!(database.taxonomy_identity().unwrap(), candidate_identity);
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
}

#[test]
fn source_and_sql_changes_invalidate_the_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = simple_session(&directory, &database);
    validate_base_import(&database, &session.session_id).unwrap();
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    let candidate_path = read_session_state(&workspace)
        .unwrap()
        .candidate
        .unwrap()
        .candidate_path;

    let extra_csv = directory.path().join("extra.csv");
    fs::write(&extra_csv, "value\nextra\n").unwrap();
    add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "extra".into(),
            path: extra_csv,
        },
    )
    .unwrap();

    assert!(!candidate_path.exists());
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
    assert!(!workspace.join(STAGING_DATABASE).exists());

    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();
    validate_base_import(&database, &session.session_id).unwrap();
    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
}

#[test]
fn removes_csv_source_without_changing_other_sources() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let sqlite_path = directory.path().join("source.db");
    Connection::open(&sqlite_path)
        .unwrap()
        .execute_batch("CREATE TABLE taxa (taxon_id INTEGER PRIMARY KEY)")
        .unwrap();
    let csv_path = directory.path().join("names.csv");
    fs::write(&csv_path, "name\nAnimalia\n").unwrap();
    let session = create_base_import_session(&database).unwrap();
    add_base_import_sqlite_source(
        &database,
        &AddBaseImportSqliteSourceRequest {
            session_id: session.session_id.clone(),
            path: sqlite_path,
        },
    )
    .unwrap();
    add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "names".into(),
            path: csv_path,
        },
    )
    .unwrap();

    let result = remove_base_import_source(
        &database,
        &RemoveBaseImportSourceRequest {
            session_id: session.session_id.clone(),
            source_alias: "names".into(),
        },
    )
    .unwrap();

    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].source_alias, "main");
    assert_eq!(result.session_revision, 3);
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    let connection = Connection::open(workspace.join(SOURCE_DATABASE)).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'taxa'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'names'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn removes_sqlite_source_without_changing_csv_source() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let sqlite_path = directory.path().join("source.db");
    Connection::open(&sqlite_path)
        .unwrap()
        .execute_batch("CREATE TABLE taxa (taxon_id INTEGER PRIMARY KEY)")
        .unwrap();
    let csv_path = directory.path().join("names.csv");
    fs::write(&csv_path, "name\nAnimalia\n").unwrap();
    let session = create_base_import_session(&database).unwrap();
    add_base_import_sqlite_source(
        &database,
        &AddBaseImportSqliteSourceRequest {
            session_id: session.session_id.clone(),
            path: sqlite_path,
        },
    )
    .unwrap();
    add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "names".into(),
            path: csv_path,
        },
    )
    .unwrap();

    let result = remove_base_import_source(
        &database,
        &RemoveBaseImportSourceRequest {
            session_id: session.session_id.clone(),
            source_alias: "main".into(),
        },
    )
    .unwrap();

    assert_eq!(result.sources.len(), 1);
    assert_eq!(result.sources[0].source_alias, "names");
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    let connection = Connection::open(workspace.join(SOURCE_DATABASE)).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT name FROM names", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "Animalia"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name = 'taxa'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn source_removal_invalidates_candidate_and_rejects_busy_or_missing_source() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = simple_session(&directory, &database);
    validate_base_import(&database, &session.session_id).unwrap();
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    let candidate_path = read_session_state(&workspace)
        .unwrap()
        .candidate
        .as_ref()
        .unwrap()
        .candidate_path
        .clone();
    let session_mutex = session_lock(&session.session_id).unwrap();
    let guard = session_mutex.lock().unwrap();
    let busy_error = remove_base_import_source(
        &database,
        &RemoveBaseImportSourceRequest {
            session_id: session.session_id.clone(),
            source_alias: "source_taxa".into(),
        },
    )
    .unwrap_err();
    assert!(busy_error.to_string().contains("session is busy"));
    drop(guard);

    let missing_error = remove_base_import_source(
        &database,
        &RemoveBaseImportSourceRequest {
            session_id: session.session_id.clone(),
            source_alias: "missing".into(),
        },
    )
    .unwrap_err();
    assert!(
        missing_error
            .to_string()
            .contains("base import source missing")
    );

    let result = remove_base_import_source(
        &database,
        &RemoveBaseImportSourceRequest {
            session_id: session.session_id.clone(),
            source_alias: "source_taxa".into(),
        },
    )
    .unwrap();
    assert_eq!(result.session_revision, 3);
    assert!(result.sources.is_empty());
    assert!(!candidate_path.exists());
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
    assert!(!workspace.join(STAGING_DATABASE).exists());
}

#[test]
fn external_staging_change_blocks_apply_and_preserves_taxonomy() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let original_identity = database.taxonomy_identity().unwrap();
    let session = simple_session(&directory, &database);
    validate_base_import(&database, &session.session_id).unwrap();
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    Connection::open(workspace.join(STAGING_DATABASE))
        .unwrap()
        .execute(
            "UPDATE taxon_names SET source = 'changed' WHERE name_id = 1",
            [],
        )
        .unwrap();

    let error = apply_base_import(&database, &session.session_id).unwrap_err();

    assert!(error.to_string().contains("fingerprint is stale"));
    assert_eq!(database.taxonomy_identity().unwrap(), original_identity);
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
}

#[test]
fn large_name_build_failure_discards_candidate_and_preserves_taxonomy() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let original_identity = database.taxonomy_identity().unwrap();
    let session = create_base_import_session(&database).unwrap();
    let sql = SIMPLE_IMPORT_SQL
            .replace(
                "INSERT INTO base.taxa\nSELECT CAST(taxon_id AS INTEGER), NULL, CAST(rank AS INTEGER), geological_range\nFROM main.source_taxa;",
                "INSERT INTO base.taxa VALUES (101, NULL, 1, NULL);",
            )
            .replace(
                "INSERT INTO base.taxon_names\nSELECT 1, CAST(taxon_id AS INTEGER), 1, name, NULL, NULL, 'test'\nFROM main.source_taxa;",
                r#"
                INSERT INTO base.taxon_names
                VALUES (1, 101, 1, 'Animalia', NULL, NULL, 'test');
                WITH RECURSIVE values_cte(value) AS (
                    VALUES (1)
                    UNION ALL
                    SELECT value + 1 FROM values_cte WHERE value <= 10000
                )
                INSERT INTO base.taxon_names (
                    name_id, taxon_id, name_type, name,
                    normalized_name, authority_year, source
                )
                SELECT
                    value + 1,
                    101,
                    2,
                    CASE WHEN value = 10001 THEN ' Alias 1 ' ELSE 'Alias ' || value END,
                    NULL,
                    NULL,
                    'test'
                FROM values_cte;
                "#,
            );
    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            session_id: session.session_id.clone(),
            sql,
        },
    )
    .unwrap();

    let validation = validate_base_import(&database, &session.session_id).unwrap();
    let workspace = session_workspace(&database, &session.session_id).unwrap();

    assert!(!validation.can_apply);
    assert!(
        validation
            .errors
            .iter()
            .any(|issue| issue.code == "taxonomy_validation_failed")
    );
    assert!(read_session_state(&workspace).unwrap().candidate.is_none());
    assert!(!workspace.join(CANDIDATE_BUILD_DATABASE).exists());
    assert_eq!(database.taxonomy_identity().unwrap(), original_identity);
}

#[test]
fn csv_failure_rolls_back_the_entire_source_table() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let csv_path = directory.path().join("broken.csv");
    fs::write(&csv_path, "name,value\nvalid,1\nbroken\n").unwrap();
    let session = create_base_import_session(&database).unwrap();

    let error = add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "broken_source".into(),
            path: csv_path,
        },
    )
    .unwrap_err();
    let schema = inspect_base_import_sources(&database, &session.session_id).unwrap();

    assert!(error.to_string().contains("CSV row 3"));
    assert!(schema.is_empty());

    let mut large_csv = String::from("value\n");
    for value in 0..5_000 {
        large_csv.push_str(&format!("{value}\n"));
    }
    let large_path = directory.path().join("large.csv");
    fs::write(&large_path, large_csv).unwrap();
    add_base_import_csv_source(
        &database,
        &AddBaseImportCsvSourceRequest {
            session_id: session.session_id.clone(),
            table_name: "large_source".into(),
            path: large_path,
        },
    )
    .unwrap();
    let workspace = session_workspace(&database, &session.session_id).unwrap();
    assert_eq!(
        Connection::open(workspace.join(SOURCE_DATABASE))
            .unwrap()
            .query_row("SELECT COUNT(*) FROM large_source", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        5_000
    );
}

#[test]
fn replacement_guard_rejects_mutation_relocation_and_another_apply() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = simple_session(&directory, &database);
    validate_base_import(&database, &session.session_id).unwrap();
    let guard = database.try_taxonomy_replacement().unwrap();

    assert!(
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Blocked".into()),
                ..TaxonInputRow::default()
            }]
        )
        .unwrap_err()
        .to_string()
        .contains("replacement is in progress")
    );
    assert!(
        crate::taxonomy::execute_custom_taxonomy_sql(
            &database,
            &crate::taxonomy::CustomTaxonomySqlRequest {
                sql: "UPDATE taxa SET geological_range = 'Blocked'".into(),
                sources: Vec::new(),
                maximum_result_rows: None,
            }
        )
        .unwrap_err()
        .to_string()
        .contains("replacement is in progress")
    );
    assert!(
        database
            .relocate_taxonomy_database(&directory.path().join("relocated.db"))
            .unwrap_err()
            .to_string()
            .contains("replacement is in progress")
    );
    assert!(
        apply_base_import(&database, &session.session_id)
            .unwrap_err()
            .to_string()
            .contains("taxonomy database is busy")
    );

    drop(guard);
    apply_base_import(&database, &session.session_id).unwrap();
}

#[test]
fn failed_apply_releases_the_replacement_guard() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let session = create_base_import_session(&database).unwrap();

    assert!(apply_base_import(&database, &session.session_id).is_err());
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Available".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
}

#[test]
fn default_sql_round_trips_without_formatting() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    assert_eq!(
        get_default_base_import_sql(&database).unwrap(),
        BUILTIN_DEFAULT_BASE_IMPORT_SQL
    );
    let custom = "-- Keep exact formatting.\nSELECT  1;\n";
    save_default_base_import_sql(&database, custom).unwrap();
    assert_eq!(get_default_base_import_sql(&database).unwrap(), custom);
    assert_eq!(
        reset_default_base_import_sql(&database).unwrap(),
        BUILTIN_DEFAULT_BASE_IMPORT_SQL
    );
    assert_eq!(
        get_default_base_import_sql(&database).unwrap(),
        BUILTIN_DEFAULT_BASE_IMPORT_SQL
    );
}
