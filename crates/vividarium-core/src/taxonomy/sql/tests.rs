use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;

use super::*;
use crate::taxonomy::{TaxonInputRow, apply_rows, list_operations};

fn database() -> (tempfile::TempDir, Database) {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();
    apply_rows(
        &database,
        &[TaxonInputRow {
            kingdom: Some("Animalia".into()),
            ..TaxonInputRow::default()
        }],
    )
    .unwrap();
    (directory, database)
}

fn request(sql: &str) -> CustomTaxonomySqlRequest {
    CustomTaxonomySqlRequest {
        sql: sql.into(),
        maximum_result_rows: None,
    }
}

#[test]
fn returns_typed_results_and_only_logs_actual_mutations() {
    let (_directory, database) = database();
    let before = list_operations(&database, None, 20).unwrap().items.len();
    let query = execute_custom_taxonomy_sql(
        &database,
        &request(
            "SELECT taxon_id, rank, geological_range, CAST(NULL AS TEXT) AS missing FROM taxa",
        ),
    )
    .unwrap();
    assert_eq!(query.operation_id, None);
    assert_eq!(query.changeset_size, 0);
    assert!(query.script_saved);
    assert!(query.warnings.is_empty());
    assert_eq!(query.result_sets.len(), 1);
    assert_eq!(query.result_sets[0].rows[0][1], SqlValue::Integer(1));
    assert_eq!(query.result_sets[0].rows[0][2], SqlValue::Null);
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before
    );

    let mutation = execute_custom_taxonomy_sql(
        &database,
        &request("UPDATE taxa SET geological_range = 'Recent' RETURNING geological_range"),
    )
    .unwrap();
    assert!(mutation.operation_id.is_some());
    assert!(mutation.changeset_size > 0);
    assert!(mutation.script_saved);
    assert!(mutation.warnings.is_empty());
    assert_eq!(
        mutation.result_sets[0].rows,
        vec![vec![SqlValue::Text("Recent".into())]]
    );
    assert_eq!(mutation.messages[0].affected_rows, Some(1));
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before + 1
    );
}

#[test]
fn saves_only_successful_scripts() {
    let (_directory, database) = database();
    let initial = get_custom_taxonomy_sql(&database).unwrap();

    execute_custom_taxonomy_sql(&database, &request("SELECT 1; SELECT 2")).unwrap();
    assert_eq!(
        get_custom_taxonomy_sql(&database).unwrap(),
        "SELECT 1; SELECT 2"
    );

    execute_custom_taxonomy_sql(&database, &request("SELECT taxon_id FROM taxa")).unwrap();
    assert_eq!(
        get_custom_taxonomy_sql(&database).unwrap(),
        "SELECT taxon_id FROM taxa"
    );

    execute_custom_taxonomy_sql(&database, &request("SELECT missing FROM taxa")).unwrap_err();
    assert_eq!(
        get_custom_taxonomy_sql(&database).unwrap(),
        "SELECT taxon_id FROM taxa"
    );
    assert_ne!(get_custom_taxonomy_sql(&database).unwrap(), initial);
}

#[test]
fn multiple_queries_use_executable_statement_indexes() {
    let (_directory, database) = database();
    let result = execute_custom_taxonomy_sql(
        &database,
        &request(
            r#"
            -- first query
            SELECT 1 AS value;

            /* second query */
            SELECT 2 AS value;
            "#,
        ),
    )
    .unwrap();

    assert_eq!(
        result
            .result_sets
            .iter()
            .map(|result_set| result_set.statement_index)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(result.result_sets[0].rows, vec![vec![SqlValue::Integer(1)]]);
    assert_eq!(result.result_sets[1].rows, vec![vec![SqlValue::Integer(2)]]);
    assert_eq!(
        result
            .messages
            .iter()
            .map(|message| message.statement_index)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn mixed_scripts_create_one_operation_and_keep_statement_indexes() {
    let (_directory, database) = database();
    let before = list_operations(&database, None, 20).unwrap().items.len();

    let result = execute_custom_taxonomy_sql(
        &database,
        &request(
            "UPDATE taxa SET geological_range = 'Recent'; SELECT taxon_id, geological_range FROM taxa",
        ),
    )
    .unwrap();

    assert!(result.operation_id.is_some());
    assert_eq!(result.messages[0].statement_index, 1);
    assert_eq!(result.messages[0].affected_rows, Some(1));
    assert_eq!(result.result_sets[0].statement_index, 2);
    assert_eq!(result.messages[1].affected_rows, None);
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before + 1
    );
}

#[test]
fn a_later_statement_failure_rolls_back_the_entire_script() {
    let (_directory, database) = database();
    let saved = get_custom_taxonomy_sql(&database).unwrap();
    let before = list_operations(&database, None, 20).unwrap().items.len();

    execute_custom_taxonomy_sql(
        &database,
        &request("UPDATE taxa SET geological_range = 'Recent'; SELECT missing_column FROM taxa"),
    )
    .unwrap_err();

    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT geological_range FROM taxa", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap(),
        None
    );
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before
    );
    assert_eq!(get_custom_taxonomy_sql(&database).unwrap(), saved);
}

#[test]
fn comment_only_scripts_are_not_executable() {
    let (_directory, database) = database();

    let error =
        execute_custom_taxonomy_sql(&database, &request("-- no statement\n/* still empty */"))
            .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("at least one executable statement")
    );
}

#[test]
fn committed_sql_reports_script_save_failure_as_warning() {
    let (_directory, database) = database();
    database
        .connect_metadata()
        .unwrap()
        .execute("DROP TABLE app_metadata", [])
        .unwrap();

    let result = execute_custom_taxonomy_sql(
        &database,
        &request("UPDATE taxa SET geological_range = 'Recent'"),
    )
    .unwrap();

    assert!(result.operation_id.is_some());
    assert!(!result.script_saved);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("script could not be saved"));
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT geological_range FROM taxa LIMIT 1", [], |row| {
                row.get::<_, String>(0)
            },)
            .unwrap(),
        "Recent"
    );
}

#[test]
fn read_only_preview_stops_after_limit_plus_one_rows() {
    let connection = Connection::open_in_memory().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let function_calls = calls.clone();
    connection
        .create_scalar_function(
            "observe_row",
            1,
            FunctionFlags::SQLITE_UTF8,
            move |context| {
                function_calls.fetch_add(1, Ordering::Relaxed);
                context.get::<i64>(0)
            },
        )
        .unwrap();
    let result = execute_custom_script(
        &connection,
        r#"
            WITH RECURSIVE values_cte(value) AS (
                VALUES (1)
                UNION ALL
                SELECT value + 1 FROM values_cte WHERE value < 1000
            )
            SELECT observe_row(value) FROM values_cte;
            "#,
        2,
        custom_sql_authorizer,
    )
    .unwrap();

    assert_eq!(result.result_sets[0].rows.len(), 2);
    assert!(result.result_sets[0].truncated);
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    assert_eq!(
        result.messages[0].message,
        "statement returned more than 2 rows"
    );
}

#[test]
fn mutation_returning_runs_to_completion_after_preview_limit() {
    let (_directory, database) = database();
    apply_rows(
        &database,
        &[
            TaxonInputRow {
                kingdom: Some("Archaea".into()),
                ..TaxonInputRow::default()
            },
            TaxonInputRow {
                kingdom: Some("Bacteria".into()),
                ..TaxonInputRow::default()
            },
        ],
    )
    .unwrap();
    let mut limited = request("UPDATE taxa SET geological_range = 'Recent' RETURNING taxon_id");
    limited.maximum_result_rows = Some(2);

    let result = execute_custom_taxonomy_sql(&database, &limited).unwrap();

    assert_eq!(result.result_sets[0].rows.len(), 2);
    assert!(result.result_sets[0].truncated);
    assert_eq!(result.messages[0].affected_rows, Some(3));
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM taxa WHERE geological_range = 'Recent'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        3
    );
}

#[test]
fn csv_and_sqlite_sources_are_read_only() {
    let (directory, database) = database();
    let csv_path = directory.path().join("input.csv");
    std::fs::write(&csv_path, "name,value\nAnimalia,\n").unwrap();
    let sqlite_path = directory.path().join("source.db");
    let source = Connection::open(&sqlite_path).unwrap();
    source
        .execute_batch(
            "CREATE TABLE source_names(name TEXT); INSERT INTO source_names VALUES ('Metazoa');",
        )
        .unwrap();
    drop(source);
    add_custom_sql_input(
        &database,
        &AddSqlInputRequest {
            kind: super::SqlInputKind::Csv,
            alias: "csv_input".into(),
            path: csv_path.clone(),
        },
    )
    .unwrap();
    add_custom_sql_input(
        &database,
        &AddSqlInputRequest {
            kind: super::SqlInputKind::Sqlite,
            alias: "external".into(),
            path: sqlite_path.clone(),
        },
    )
    .unwrap();
    let result = execute_custom_taxonomy_sql(
            &database,
            &CustomTaxonomySqlRequest {
                sql: "SELECT csv_input.name, csv_input.value, external.source_names.name FROM csv_input CROSS JOIN external.source_names".into(),
                maximum_result_rows: None,
            },
        )
        .unwrap();
    assert_eq!(
        result.result_sets[0].rows[0],
        vec![
            SqlValue::Text("Animalia".into()),
            SqlValue::Text(String::new()),
            SqlValue::Text("Metazoa".into())
        ]
    );
    let error = execute_custom_taxonomy_sql(
        &database,
        &CustomTaxonomySqlRequest {
            sql: "DELETE FROM external.source_names".into(),
            maximum_result_rows: None,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("not authorized"));
    assert_eq!(
        Connection::open(&sqlite_path)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM source_names", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let schema = inspect_sql_data_source(
        &SqlDataSource::Csv {
            alias: "csv_input".into(),
            path: csv_path,
        },
        b',',
    )
    .unwrap();
    assert_eq!(schema.objects[0].columns[0].name, "name");
}

#[test]
fn csv_load_uses_one_atomic_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.csv");
    std::fs::write(&path, "name,value\nvalid,1\nbroken\n").unwrap();
    let mut connection = Connection::open_in_memory().unwrap();

    let error = load_csv_table(&mut connection, "source", &path, b',').unwrap_err();

    assert!(error.to_string().contains("CSV row 3"));
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_temp_schema WHERE name = 'source'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );

    let mut large_csv = String::from("value\n");
    for value in 0..5_000 {
        large_csv.push_str(&format!("{value}\n"));
    }
    let large_path = directory.path().join("large.csv");
    std::fs::write(&large_path, large_csv).unwrap();
    load_csv_table(&mut connection, "large_source", &large_path, b',').unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM large_source", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        5_000
    );
}

#[test]
fn custom_sql_database_schemas_include_main_taxonomy_tables() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path().join("metadata.db")).unwrap();

    let schemas = list_custom_sql_database_schemas(&database).unwrap();

    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].alias, "main");
    assert!(
        schemas[0]
            .objects
            .iter()
            .any(|object| object.name == "taxa")
    );
    assert!(
        schemas[0]
            .objects
            .iter()
            .any(|object| object.name == "taxon_names")
    );
}

#[test]
fn rejects_control_and_internal_write_statements() {
    let (_directory, database) = database();
    for sql in [
        "BEGIN",
        "ATTACH DATABASE 'other.db' AS other",
        "DELETE FROM operations",
        "DROP TABLE taxa",
        "PRAGMA writable_schema = ON",
    ] {
        assert!(
            execute_custom_taxonomy_sql(&database, &request(sql))
                .unwrap_err()
                .to_string()
                .contains("not authorized"),
            "{sql}"
        );
    }
}

#[test]
fn streams_one_query_to_csv() {
    let (directory, database) = database();
    let destination = directory.path().join("query.csv");
    let result = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "SELECT rank, name FROM taxa JOIN taxon_names USING (taxon_id)".into(),
            statement_index: 1,
            destination_path: destination.clone(),
        },
    )
    .unwrap();
    assert_eq!(result.row_count, 1);
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "rank,name\n1,Animalia\n"
    );
}

#[test]
fn exports_a_complete_result_beyond_the_preview_limit() {
    let (directory, database) = database();
    let sql = r#"
        WITH RECURSIVE values_cte(value) AS (
            VALUES (1)
            UNION ALL
            SELECT value + 1 FROM values_cte WHERE value < 1500
        )
        SELECT value FROM values_cte
    "#;
    let mut preview_request = request(sql);
    preview_request.maximum_result_rows = Some(20);
    let preview = execute_custom_taxonomy_sql(&database, &preview_request).unwrap();
    assert_eq!(preview.result_sets[0].rows.len(), 20);
    assert!(preview.result_sets[0].truncated);

    let destination = directory.path().join("complete-query.csv");
    let exported = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: sql.into(),
            statement_index: 1,
            destination_path: destination.clone(),
        },
    )
    .unwrap();

    assert_eq!(exported.row_count, 1500);
    assert_eq!(
        std::fs::read_to_string(destination)
            .unwrap()
            .lines()
            .count(),
        1501
    );
}

#[test]
fn exports_only_the_selected_read_only_statement() {
    let (directory, database) = database();
    let destination = directory.path().join("second-statement.csv");
    let result = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "UPDATE taxa SET geological_range = 'Changed'; SELECT name FROM taxon_names ORDER BY name".into(),
            statement_index: 2,
            destination_path: destination.clone(),
        },
    )
    .unwrap();

    assert_eq!(result.row_count, 1);
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "name\nAnimalia\n"
    );
    assert_eq!(
        database
            .connect_taxonomy()
            .unwrap()
            .query_row("SELECT geological_range FROM taxa", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .unwrap(),
        None
    );
}

#[test]
fn empty_query_export_contains_only_the_header() {
    let (directory, database) = database();
    let destination = directory.path().join("empty-query.csv");
    let result = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "SELECT taxon_id, rank FROM taxa WHERE 1 = 0".into(),
            statement_index: 1,
            destination_path: destination.clone(),
        },
    )
    .unwrap();

    assert_eq!(result.row_count, 0);
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "taxon_id,rank\n"
    );
}

#[test]
fn export_rejects_mutations_and_invalid_statement_indexes() {
    let (directory, database) = database();
    let mutation_destination = directory.path().join("mutation.csv");
    let mutation_error = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "UPDATE taxa SET geological_range = 'Changed' RETURNING taxon_id".into(),
            statement_index: 1,
            destination_path: mutation_destination,
        },
    )
    .unwrap_err();
    assert!(mutation_error.to_string().contains("read-only query"));

    let missing_destination = directory.path().join("missing.csv");
    let missing_error = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "SELECT taxon_id FROM taxa; SELECT name FROM taxon_names".into(),
            statement_index: 99,
            destination_path: missing_destination,
        },
    )
    .unwrap_err();
    assert!(
        missing_error
            .to_string()
            .contains("statement 99 does not exist")
    );

    let zero_destination = directory.path().join("zero.csv");
    let zero_error = export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "SELECT taxon_id FROM taxa".into(),
            statement_index: 0,
            destination_path: zero_destination,
        },
    )
    .unwrap_err();
    assert!(zero_error.to_string().contains("must be at least 1"));
}

#[test]
fn configured_csv_delimiter_controls_sql_sources_and_exports() {
    let (directory, database) = database();
    crate::general::update_general_settings(
        &database,
        &crate::general::GeneralSettings {
            csv_delimiter: "|".into(),
            ..crate::general::GeneralSettings::default()
        },
    )
    .unwrap();
    let source = directory.path().join("source.csv");
    std::fs::write(&source, "name|value\nAnimalia|Recent\n").unwrap();
    add_custom_sql_input(
        &database,
        &AddSqlInputRequest {
            kind: super::SqlInputKind::Csv,
            alias: "csv_input".into(),
            path: source,
        },
    )
    .unwrap();
    let result = execute_custom_taxonomy_sql(
        &database,
        &CustomTaxonomySqlRequest {
            sql: "SELECT name, value FROM csv_input".into(),
            maximum_result_rows: None,
        },
    )
    .unwrap();
    assert_eq!(
        result.result_sets[0].rows[0],
        vec![
            SqlValue::Text("Animalia".into()),
            SqlValue::Text("Recent".into())
        ]
    );
    let destination = directory.path().join("query.csv");
    export_custom_taxonomy_query(
        &database,
        &CustomTaxonomySqlExportRequest {
            sql: "SELECT name, value FROM csv_input".into(),
            statement_index: 1,
            destination_path: destination.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "name|value\nAnimalia|Recent\n"
    );
}
