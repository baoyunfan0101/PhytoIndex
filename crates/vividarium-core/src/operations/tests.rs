use super::*;

#[test]
fn pages_summaries_and_audit_without_loading_nested_rows() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
                CREATE TABLE operations (
                    operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind TEXT NOT NULL,
                    source TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    total_items INTEGER NOT NULL,
                    succeeded_items INTEGER NOT NULL,
                    failed_items INTEGER NOT NULL,
                    rollbackable INTEGER NOT NULL,
                    has_formatted_input INTEGER NOT NULL
                );
                CREATE TABLE operation_audit_rows (
                    operation_id INTEGER NOT NULL,
                    sequence INTEGER NOT NULL,
                    entity_type TEXT NOT NULL,
                    entity_id TEXT,
                    action TEXT NOT NULL,
                    before_json TEXT,
                    after_json TEXT,
                    succeeded INTEGER NOT NULL,
                    message TEXT NOT NULL,
                    PRIMARY KEY (operation_id, sequence)
                );
                "#,
        )
        .unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    for index in 1..=2 {
        let operation_id = insert_operation(
            &transaction,
            NewOperation {
                kind: "test",
                source: "test",
                total_items: 2,
                succeeded_items: 2,
                failed_items: 0,
                rollbackable: true,
                has_formatted_input: false,
            },
        )
        .unwrap();
        for sequence in 1..=2 {
            insert_audit_row(
                &transaction,
                operation_id,
                NewAuditRow {
                    sequence,
                    entity_type: "test",
                    entity_id: Some(index.to_string()),
                    action: "change",
                    before_json: None,
                    after_json: Some(serde_json::json!({ "index": index, "sequence": sequence })),
                    succeeded: true,
                    message: "applied",
                },
            )
            .unwrap();
        }
    }
    transaction.commit().unwrap();
    let first = list_operations(&connection, None, 1).unwrap();
    assert_eq!(first.items.len(), 1);
    assert!(first.next_cursor.is_some());
    let second = list_operations(&connection, first.next_cursor.as_deref(), 1).unwrap();
    assert_eq!(second.items.len(), 1);
    let audit = list_operation_audit(&connection, 1, None, 1).unwrap();
    assert_eq!(
        audit.items[0].after_json,
        Some(serde_json::json!({ "index": 1, "sequence": 1 }))
    );
    assert!(audit.next_cursor.is_some());
    let next_audit = list_operation_audit(&connection, 1, audit.next_cursor.as_deref(), 1).unwrap();
    assert_eq!(next_audit.items[0].sequence, 2);
    assert_eq!(next_audit.next_cursor, None);

    let mut output = Vec::new();
    write_operation_audit(&connection, Some(&[2, 1]), b'|', &mut output).unwrap();
    let exported = String::from_utf8(output).unwrap();
    let mut csv = csv::ReaderBuilder::new()
        .delimiter(b'|')
        .from_reader(exported.as_bytes());
    assert_eq!(
        csv.headers().unwrap().iter().collect::<Vec<_>>(),
        AUDIT_COLUMNS
    );
    let records = csv.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 4);
    assert_eq!(&records[0][0], "1");
    assert_eq!(&records[2][0], "2");
}
