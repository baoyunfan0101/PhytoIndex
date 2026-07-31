use super::*;
use crate::taxonomy::{AddSqlInputRequest, RemoveSqlInputRequest, SqlInputKind};
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
FROM source_taxa;
INSERT INTO base.taxon_names
SELECT 1, CAST(taxon_id AS INTEGER), 1, name, NULL, NULL, 'test'
FROM source_taxa;
COMMIT;
DETACH DATABASE base;
"#;

fn add_simple_input(directory: &tempfile::TempDir, database: &Database) {
    let csv_path = directory.path().join("simple-source.csv");
    fs::write(
        &csv_path,
        "taxon_id,rank,name,geological_range\n101,1,Animalia,Recent\n",
    )
    .unwrap();
    add_base_import_input(
        database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Csv,
            alias: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();
}

fn execute_simple(database: &Database) -> BaseImportExecutionResult {
    execute_base_import_sql(
        database,
        &ExecuteBaseImportSqlRequest {
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap()
}

#[test]
fn persistent_inputs_and_successful_sql_survive_apply_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let metadata_path = directory.path().join("metadata.db");
    let database = Database::open(&metadata_path).unwrap();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Old kingdom".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    let old_identity = database.taxonomy_identity().unwrap();
    add_simple_input(&directory, &database);

    let execution = execute_simple(&database);
    assert!(execution.statements_executed > 0);
    assert_eq!(execution.messages.len(), execution.statements_executed);
    let validation = validate_base_import(&database).unwrap();
    assert!(validation.can_apply, "{:?}", validation.errors);
    let result = apply_base_import(&database).unwrap();

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
    assert_eq!(list_base_import_inputs(&database).unwrap().len(), 1);
    assert_eq!(get_base_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
    let workspace = workspace(&database).unwrap();
    assert!(!workspace.join(STAGING_DATABASE).exists());
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
    assert!(!workspace.join(VALIDATION_STATE).exists());
    drop(database);

    let database = Database::open(metadata_path).unwrap();
    assert_eq!(list_base_import_inputs(&database).unwrap().len(), 1);
    assert_eq!(get_base_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
}

#[test]
fn execution_success_is_saved_even_when_validation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let invalid_sql =
        SIMPLE_IMPORT_SQL.replace("COMMIT;", "DELETE FROM base.taxon_names;\nCOMMIT;");

    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            sql: invalid_sql.clone(),
        },
    )
    .unwrap();
    let validation = validate_base_import(&database).unwrap();

    assert!(!validation.can_apply);
    assert!(apply_base_import(&database).is_err());
    assert_eq!(get_base_import_sql(&database).unwrap(), invalid_sql);
}

#[test]
fn failed_execution_does_not_replace_saved_sql() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);

    let error = execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            sql: "SELECT FROM".into(),
        },
    )
    .unwrap_err();

    assert!(!error.to_string().is_empty());
    assert_eq!(get_base_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
}

#[test]
fn source_removal_invalidates_staging_validation_and_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);
    assert!(validate_base_import(&database).unwrap().can_apply);
    let workspace = workspace(&database).unwrap();
    assert!(workspace.join(STAGING_DATABASE).exists());
    assert!(workspace.join(CANDIDATE_DATABASE).exists());

    let result = remove_base_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "source_taxa".into(),
        },
    )
    .unwrap();

    assert!(result.inputs.is_empty());
    assert!(!workspace.join(STAGING_DATABASE).exists());
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
    assert!(!workspace.join(VALIDATION_STATE).exists());
}

#[test]
fn source_removal_rejects_busy_workspace_and_missing_alias() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let workspace_mutex = workspace_mutex(&database).unwrap();
    let guard = lock_workspace(&workspace_mutex).unwrap();
    let busy = remove_base_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "source_taxa".into(),
        },
    )
    .unwrap_err();
    assert!(busy.to_string().contains("workspace is busy"));
    drop(guard);
    assert_eq!(list_base_import_inputs(&database).unwrap().len(), 1);

    let missing = remove_base_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "missing".into(),
        },
    )
    .unwrap_err();
    assert!(missing.to_string().contains("SQL input missing"));
    assert_eq!(list_base_import_inputs(&database).unwrap().len(), 1);
}

#[test]
fn built_in_sql_reads_a_named_sqlite_input() {
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
            INSERT INTO chinese VALUES (10, 1, 'Animals zh', 'test');
            "#,
        )
        .unwrap();
    drop(source);
    add_base_import_input(
        &database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Sqlite,
            alias: "biolib".into(),
            path: source_path,
        },
    )
    .unwrap();

    execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            sql: get_base_import_sql(&database).unwrap(),
        },
    )
    .unwrap();
    let validation = validate_base_import(&database).unwrap();
    assert!(validation.can_apply, "{:?}", validation.errors);
    assert_eq!(validation.taxa_count, 1);
}

#[test]
fn base_import_rejects_unregistered_attachments() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let error = execute_base_import_sql(
        &database,
        &ExecuteBaseImportSqlRequest {
            sql: "ATTACH DATABASE 'other.db' AS other".into(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("not authorized"));
}

#[test]
fn external_staging_change_invalidates_apply() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);
    assert!(validate_base_import(&database).unwrap().can_apply);
    let workspace = workspace(&database).unwrap();
    Connection::open(workspace.join(STAGING_DATABASE))
        .unwrap()
        .execute(
            "UPDATE taxon_names SET source = 'changed' WHERE name_id = 1",
            [],
        )
        .unwrap();

    let error = apply_base_import(&database).unwrap_err();
    assert!(error.to_string().contains("fingerprint is stale"));
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
}
