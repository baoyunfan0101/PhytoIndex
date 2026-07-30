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
        sources: Vec::new(),
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
    assert_eq!(query.result_sets.len(), 1);
    assert_eq!(query.result_sets[0].rows[0][1], SqlValue::Integer(1));
    assert_eq!(query.result_sets[0].rows[0][2], SqlValue::Null);
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before
    );

    let mutation = execute_custom_taxonomy_sql(
        &database,
        &request(
            r#"
                UPDATE taxa SET geological_range = 'Recent';
                SELECT geological_range FROM taxa;
                "#,
        ),
    )
    .unwrap();
    assert!(mutation.operation_id.is_some());
    assert!(mutation.changeset_size > 0);
    assert_eq!(
        mutation.result_sets[0].rows,
        vec![vec![SqlValue::Text("Recent".into())]]
    );
    assert_eq!(mutation.messages[0].affected_rows, Some(1));
    assert_eq!(mutation.messages[1].affected_rows, None);
    assert_eq!(
        list_operations(&database, None, 20).unwrap().items.len(),
        before + 1
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
    let sources = vec![
        SqlDataSource::Csv {
            alias: "csv_input".into(),
            path: csv_path.clone(),
        },
        SqlDataSource::Sqlite {
            alias: "external".into(),
            path: sqlite_path.clone(),
        },
    ];
    let result = execute_custom_taxonomy_sql(
            &database,
            &CustomTaxonomySqlRequest {
                sql: "SELECT csv_input.name, csv_input.value, external.source_names.name FROM csv_input CROSS JOIN external.source_names".into(),
                sources: sources.clone(),
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
            sources,
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
    let schema = inspect_sql_data_source(&SqlDataSource::Csv {
        alias: "csv_input".into(),
        path: csv_path,
    })
    .unwrap();
    assert_eq!(schema.objects[0].columns[0].name, "name");
}

#[test]
fn csv_load_uses_one_atomic_transaction() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("broken.csv");
    std::fs::write(&path, "name,value\nvalid,1\nbroken\n").unwrap();
    let mut connection = Connection::open_in_memory().unwrap();

    let error = load_csv_table(&mut connection, "source", &path).unwrap_err();

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
    load_csv_table(&mut connection, "large_source", &large_path).unwrap();
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
            sources: Vec::new(),
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
