use super::*;
use crate::taxonomy::{AddSqlInputRequest, RemoveSqlInputRequest, SqlInputKind};
use crate::taxonomy::{TaxonInputRow, apply_rows, get_taxon_detail, list_operations};

const SIMPLE_IMPORT_SQL: &str = r#"
ATTACH DATABASE 'vividarium_sql_import.db' AS sql_import;
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;
CREATE TABLE sql_import.taxa (
    taxon_id INTEGER PRIMARY KEY,
    parent_taxon_id INTEGER,
    rank INTEGER NOT NULL,
    geological_range TEXT,
    CHECK (rank BETWEEN 1 AND 5),
    FOREIGN KEY (parent_taxon_id)
        REFERENCES taxa(taxon_id) ON DELETE RESTRICT
);
CREATE TABLE sql_import.taxon_names (
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
INSERT INTO sql_import.taxa
SELECT CAST(taxon_id AS INTEGER), NULL, CAST(rank AS INTEGER), geological_range
FROM source_taxa;
INSERT INTO sql_import.taxon_names
SELECT 1, CAST(taxon_id AS INTEGER), 1, name, NULL, NULL, 'test'
FROM source_taxa;
COMMIT;
DETACH DATABASE sql_import;
"#;

fn add_simple_input(directory: &tempfile::TempDir, database: &Database) {
    let csv_path = directory.path().join("simple-source.csv");
    fs::write(
        &csv_path,
        "taxon_id,rank,name,geological_range\n101,1,Animalia,Recent\n",
    )
    .unwrap();
    add_sql_import_input(
        database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Csv,
            alias: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();
}

fn execute_simple(database: &Database) -> SqlImportExecutionResult {
    execute_sql_import_sql(
        database,
        &ValidateSqlImportRequest {
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap()
}

#[test]
fn validate_executes_sql_and_builds_the_candidate_in_one_request() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);

    let result = validate_sql_import(
        &database,
        &ValidateSqlImportRequest {
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();

    assert!(result.execution.statements_executed > 0);
    assert!(result.execution.script_saved);
    assert!(result.can_apply);
    assert!(result.validation.valid);
    assert!(result.validation.can_apply);
    assert!(result.warnings.is_empty());
    let workspace = workspace(&database).unwrap();
    assert!(workspace.join(CANDIDATE_DATABASE).is_file());
    assert!(workspace.join(VALIDATION_STATE).is_file());
}

#[test]
fn sql_import_uses_the_configured_csv_delimiter() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    crate::general::update_general_settings(
        &database,
        &crate::general::GeneralSettings {
            csv_delimiter: ";".into(),
            ..crate::general::GeneralSettings::default()
        },
    )
    .unwrap();
    let csv_path = directory.path().join("semicolon-source.csv");
    fs::write(
        &csv_path,
        "taxon_id;rank;name;geological_range\n101;1;Animalia;Recent\n",
    )
    .unwrap();
    add_sql_import_input(
        &database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Csv,
            alias: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();
    let result = validate_sql_import(
        &database,
        &ValidateSqlImportRequest {
            sql: SIMPLE_IMPORT_SQL.into(),
        },
    )
    .unwrap();
    assert!(result.validation.valid);
    assert_eq!(result.validation.taxa_count, 1);
}

#[test]
fn validate_reports_real_stages_and_sql_statement_progress() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let mut progress = Vec::new();

    let result = validate_sql_import_with_progress(
        &database,
        &ValidateSqlImportRequest {
            sql: SIMPLE_IMPORT_SQL.into(),
        },
        &mut |event| progress.push(event),
    )
    .unwrap();

    assert!(result.can_apply);
    let stages = progress
        .iter()
        .map(|event| event.stage.as_str())
        .collect::<Vec<_>>();
    let expected = [
        PREPARING_INPUT_SOURCES,
        EXECUTING_SQL,
        BUILDING_STAGING_DATABASE,
        NORMALIZING_NAMES,
        BUILDING_CANDIDATE_TAXA,
        BUILDING_CANDIDATE_NAMES,
        VALIDATING_TAXONOMY,
        READY_TO_APPLY,
    ];
    let mut previous = 0;
    for stage in expected {
        let index = stages[previous..]
            .iter()
            .position(|candidate| *candidate == stage)
            .map(|index| index + previous)
            .unwrap_or_else(|| panic!("missing progress stage {stage}: {stages:?}"));
        previous = index;
    }
    let sql_events = progress
        .iter()
        .filter(|event| event.stage == EXECUTING_SQL)
        .collect::<Vec<_>>();
    assert!(!sql_events.is_empty());
    assert_eq!(sql_events[0].statement_index, Some(1));
    assert_eq!(
        sql_events[0].statement_total,
        Some(result.execution.statements_executed as u64)
    );
    assert!(progress.iter().any(|event| {
        event.stage == NORMALIZING_NAMES && event.current == event.total && event.total == Some(1)
    }));
    assert!(progress.iter().any(|event| {
        event.stage == BUILDING_CANDIDATE_NAMES
            && event.current == event.total
            && event.total == Some(1)
    }));
}

#[test]
fn sql_statement_count_uses_sqlite_statement_boundaries() {
    assert_eq!(count_sql_statements("SELECT ';'; SELECT 2;").unwrap(), 2);
    assert_eq!(count_sql_statements("SELECT 1").unwrap(), 1);
}

#[test]
fn validate_stops_after_sql_execution_failure() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();

    let error = validate_sql_import(
        &database,
        &ValidateSqlImportRequest {
            sql: "SELECT * FROM missing_source;".into(),
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("missing_source"));
    let workspace = workspace(&database).unwrap();
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
    assert!(!workspace.join(VALIDATION_STATE).exists());
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
    assert!(execution.script_saved);
    assert!(execution.warnings.is_empty());
    let validation = validate_sql_import_candidate(&database).unwrap();
    assert!(validation.can_apply, "{:?}", validation.errors);
    let result = apply_sql_import(&database).unwrap();

    assert_eq!(result.metadata.taxa_count, 1);
    assert!(result.warnings.is_empty());
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
    assert_eq!(list_sql_import_inputs(&database).unwrap().len(), 1);
    assert_eq!(get_sql_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
    let workspace = workspace(&database).unwrap();
    assert!(!workspace.join(STAGING_DATABASE).exists());
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
    assert!(!workspace.join(VALIDATION_STATE).exists());
    drop(database);

    let database = Database::open(metadata_path).unwrap();
    assert_eq!(list_sql_import_inputs(&database).unwrap().len(), 1);
    assert_eq!(get_sql_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
}

#[test]
fn execution_success_is_saved_even_when_validation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let invalid_sql =
        SIMPLE_IMPORT_SQL.replace("COMMIT;", "DELETE FROM sql_import.taxon_names;\nCOMMIT;");

    execute_sql_import_sql(
        &database,
        &ValidateSqlImportRequest {
            sql: invalid_sql.clone(),
        },
    )
    .unwrap();
    let validation = validate_sql_import_candidate(&database).unwrap();

    assert!(!validation.valid);
    assert!(!validation.can_apply);
    assert_eq!(validation.errors[0].code, "invalid_sci_name_count");
    assert_eq!(validation.errors[0].taxon_id, Some(101));
    assert!(apply_sql_import(&database).is_err());
    assert_eq!(get_sql_import_sql(&database).unwrap(), invalid_sql);
}

#[test]
fn taxonomy_validation_failure_is_a_structured_result() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let invalid_sql = SIMPLE_IMPORT_SQL.replace(
        "COMMIT;",
        r#"
INSERT INTO sql_import.taxa (taxon_id, parent_taxon_id, rank)
VALUES (202, 101, 1);
INSERT INTO sql_import.taxon_names (name_id, taxon_id, name_type, name)
VALUES (2, 202, 1, 'Second kingdom');
COMMIT;"#,
    );

    let mut progress = Vec::new();
    let result = validate_sql_import_with_progress(
        &database,
        &ValidateSqlImportRequest { sql: invalid_sql },
        &mut |event| progress.push(event),
    )
    .unwrap();

    assert!(!result.validation.valid);
    assert!(!result.validation.can_apply);
    assert!(!result.can_apply);
    assert_eq!(result.validation.total_error_count, 1);
    assert_eq!(result.validation.errors[0].code, "kingdom_has_parent");
    assert_eq!(result.validation.errors[0].taxon_id, Some(202));
    assert_eq!(result.validation.errors[0].related_taxon_id, Some(101));
    assert_eq!(
        result.validation.errors[0].message,
        "Kingdom taxon 202 must be a root taxon."
    );
    assert!(
        progress
            .iter()
            .any(|event| event.stage == VALIDATING_TAXONOMY)
    );
    assert_eq!(progress.last().unwrap().stage, VALIDATION_FAILED);
}

#[test]
fn normalized_name_conflicts_are_validation_results() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let invalid_sql = SIMPLE_IMPORT_SQL.replace(
        "COMMIT;",
        r#"
INSERT INTO sql_import.taxon_names (name_id, taxon_id, name_type, name)
VALUES (2, 101, 2, 'Animalia  old'),
       (3, 101, 2, 'Animalia old');
COMMIT;"#,
    );

    let result =
        validate_sql_import(&database, &ValidateSqlImportRequest { sql: invalid_sql }).unwrap();

    assert!(!result.validation.valid);
    assert_eq!(result.validation.errors[0].code, "duplicate_canonical_name");
    assert_eq!(result.validation.errors[0].taxon_id, Some(101));
}

#[test]
fn failed_execution_does_not_replace_saved_sql() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);

    let error = execute_sql_import_sql(
        &database,
        &ValidateSqlImportRequest {
            sql: "SELECT FROM".into(),
        },
    )
    .unwrap_err();

    assert!(!error.to_string().is_empty());
    assert_eq!(get_sql_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
}

#[test]
fn failed_execution_restores_existing_staging_and_validation() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);
    assert!(validate_sql_import_candidate(&database).unwrap().can_apply);

    execute_sql_import_sql(
        &database,
        &ValidateSqlImportRequest {
            sql: "SELECT FROM".into(),
        },
    )
    .unwrap_err();

    assert!(validate_sql_import_candidate(&database).unwrap().can_apply);
    assert_eq!(get_sql_import_sql(&database).unwrap(), SIMPLE_IMPORT_SQL);
}

#[test]
fn committed_sql_import_sql_reports_script_save_failure_as_warning() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    database
        .connect_metadata()
        .unwrap()
        .execute("DROP TABLE app_metadata", [])
        .unwrap();

    let result = execute_simple(&database);

    assert!(!result.script_saved);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("script could not be saved"));
    assert!(
        workspace(&database)
            .unwrap()
            .join(STAGING_DATABASE)
            .is_file()
    );
}

#[test]
fn adding_input_returns_cleanup_warning_after_registry_commit() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let workspace = workspace(&database).unwrap();
    fs::create_dir(workspace.join(CANDIDATE_BUILD_DATABASE)).unwrap();
    let csv_path = directory.path().join("source.csv");
    fs::write(&csv_path, "taxon_id,name\n1,Animalia\n").unwrap();

    let result = add_sql_import_input(
        &database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Csv,
            alias: "source_taxa".into(),
            path: csv_path,
        },
    )
    .unwrap();

    assert_eq!(result.inputs.len(), 1);
    assert_eq!(result.input.alias, "source_taxa");
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("queued for retry"));
    let deferred = workspace.join(format!(".invalidated-{CANDIDATE_BUILD_DATABASE}"));
    fs::remove_dir(deferred).unwrap();
}

#[test]
fn source_removal_invalidates_staging_validation_and_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);
    assert!(validate_sql_import_candidate(&database).unwrap().can_apply);
    let workspace = workspace(&database).unwrap();
    assert!(workspace.join(STAGING_DATABASE).exists());
    assert!(workspace.join(CANDIDATE_DATABASE).exists());

    let result = remove_sql_import_input(
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
fn removing_input_returns_cleanup_warning_after_registry_commit() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let stored_path = database
        .connect_metadata()
        .unwrap()
        .query_row(
            "SELECT stored_path FROM sql_inputs WHERE scope = 2 AND alias = 'source_taxa'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    fs::remove_file(&stored_path).unwrap();
    fs::create_dir(&stored_path).unwrap();

    let result = remove_sql_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "source_taxa".into(),
        },
    )
    .unwrap();

    assert!(result.inputs.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("queued for retry"));
    fs::remove_dir(stored_path).unwrap();
}

#[test]
fn source_removal_rejects_busy_workspace_and_missing_alias() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    let workspace_mutex = workspace_mutex(&database).unwrap();
    let guard = lock_workspace(&workspace_mutex).unwrap();
    let busy = remove_sql_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "source_taxa".into(),
        },
    )
    .unwrap_err();
    assert!(busy.to_string().contains("workspace is busy"));
    drop(guard);
    assert_eq!(list_sql_import_inputs(&database).unwrap().len(), 1);

    let missing = remove_sql_import_input(
        &database,
        &RemoveSqlInputRequest {
            alias: "missing".into(),
        },
    )
    .unwrap_err();
    assert!(missing.to_string().contains("SQL input missing"));
    assert_eq!(list_sql_import_inputs(&database).unwrap().len(), 1);
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
                (10, NULL, 0, 60, 'Animalia', NULL, 'Recent', 'Animals'),
                (11, 10, 0, 601, 'Fallback species', NULL, 'Recent', NULL);
            INSERT INTO synonyms VALUES (10, 0, 'Metazoa', NULL);
            INSERT INTO chinese VALUES
                (10, 1, 'Animals zh', 'test'),
                (11, 1, '   ', 'ignored'),
                (11, 0, 'Fallback alias B', 'test'),
                (11, 0, 'Fallback alias A', 'test');
            "#,
        )
        .unwrap();
    drop(source);
    add_sql_import_input(
        &database,
        &AddSqlInputRequest {
            kind: SqlInputKind::Sqlite,
            alias: "biolib".into(),
            path: source_path,
        },
    )
    .unwrap();

    execute_sql_import_sql(
        &database,
        &ValidateSqlImportRequest {
            sql: get_sql_import_sql(&database).unwrap(),
        },
    )
    .unwrap();
    let staging = Connection::open(workspace(&database).unwrap().join(STAGING_DATABASE)).unwrap();
    let fallback_names = staging
        .prepare(
            "SELECT name_type, name FROM taxon_names WHERE taxon_id = 11 ORDER BY name_type, name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        fallback_names,
        vec![
            (1, "Fallback species".into()),
            (3, "Fallback alias A".into()),
            (4, "Fallback alias B".into()),
        ]
    );
    let validation = validate_sql_import_candidate(&database).unwrap();
    assert!(validation.can_apply, "{:?}", validation.errors);
    assert_eq!(validation.taxa_count, 2);
}

#[test]
fn sql_import_rejects_unregistered_attachments() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    let error = execute_sql_import_sql(
        &database,
        &ValidateSqlImportRequest {
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
    assert!(validate_sql_import_candidate(&database).unwrap().can_apply);
    let workspace = workspace(&database).unwrap();
    Connection::open(workspace.join(STAGING_DATABASE))
        .unwrap()
        .execute(
            "UPDATE taxon_names SET source = 'changed' WHERE name_id = 1",
            [],
        )
        .unwrap();

    let error = apply_sql_import(&database).unwrap_err();
    assert!(error.to_string().contains("fingerprint is stale"));
    assert!(!workspace.join(CANDIDATE_DATABASE).exists());
}

#[test]
fn apply_returns_cleanup_warning_after_taxonomy_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    add_simple_input(&directory, &database);
    execute_simple(&database);
    assert!(validate_sql_import_candidate(&database).unwrap().can_apply);
    let workspace = workspace(&database).unwrap();
    fs::create_dir(workspace.join(CANDIDATE_BUILD_DATABASE)).unwrap();

    let result = apply_sql_import(&database).unwrap();

    assert_eq!(result.metadata.taxa_count, 1);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("queued for retry"));
    fs::remove_dir(workspace.join(CANDIDATE_BUILD_DATABASE)).unwrap();
}
