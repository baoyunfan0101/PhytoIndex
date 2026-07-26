use csv::WriterBuilder;
use rusqlite::{OptionalExtension, params};

use crate::{CoreError, CoreResult, Database};

const PHOTO_OPERATION_EXPORT_COLUMNS: [&str; 9] = [
    "operation_id",
    "source",
    "applied_at",
    "root_path",
    "row_number",
    "photo_id",
    "directory_relative_path",
    "old_filename",
    "new_filename",
];

pub fn export_photo_operation_csv(database: &Database, operation_id: i64) -> CoreResult<String> {
    validate_operation_id(operation_id)?;
    let connection = database.connect()?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM photo_operations WHERE operation_id = ?",
            [operation_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(CoreError::NotFound(format!(
            "photo operation {operation_id}"
        )));
    }
    export_photo_operations(&connection, Some(operation_id))
}

pub fn export_all_photo_operations_csv(database: &Database) -> CoreResult<String> {
    export_photo_operations(&database.connect()?, None)
}

fn export_photo_operations(
    connection: &rusqlite::Connection,
    operation_id: Option<i64>,
) -> CoreResult<String> {
    let mut writer = WriterBuilder::new().delimiter(b'|').from_writer(Vec::new());
    writer.write_record(PHOTO_OPERATION_EXPORT_COLUMNS)?;
    let filter = if operation_id.is_some() {
        "WHERE photo_operations.operation_id = ?1"
    } else {
        ""
    };
    let mut statement = connection.prepare(&format!(
        r#"
        SELECT photo_operations.operation_id,
               photo_operations.source,
               photo_operations.applied_at,
               photo_operations.root_path,
               photo_operation_items.row_number,
               photo_operation_items.photo_id,
               photo_operation_items.directory_relative_path,
               photo_operation_items.old_filename,
               photo_operation_items.new_filename
        FROM photo_operations
        JOIN photo_operation_items USING (operation_id)
        {filter}
        ORDER BY photo_operations.operation_id, photo_operation_items.row_number
        "#
    ))?;
    let mut rows = match operation_id {
        Some(operation_id) => statement.query(params![operation_id])?,
        None => statement.query([])?,
    };
    while let Some(row) = rows.next()? {
        writer.write_record([
            row.get::<_, i64>(0)?.to_string(),
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?.to_string(),
            row.get::<_, i64>(5)?.to_string(),
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ])?;
    }
    finish_csv(writer, "photo operation export")
}

fn validate_operation_id(operation_id: i64) -> CoreResult<()> {
    if operation_id <= 0 {
        return Err(CoreError::InvalidArgument(
            "operation id must be positive".into(),
        ));
    }
    Ok(())
}

fn finish_csv(writer: csv::Writer<Vec<u8>>, description: &str) -> CoreResult<String> {
    let bytes = writer.into_inner().map_err(|error| error.into_error())?;
    String::from_utf8(bytes).map_err(|error| {
        CoreError::InvalidArgument(format!("invalid UTF-8 {description}: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use csv::ReaderBuilder;
    use rusqlite::params;

    use super::*;

    #[test]
    fn exports_one_or_all_operations_with_one_audit_header() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let connection = database.connect().unwrap();
        for (source, root_path, old_filename, new_filename) in [
            ("manual_rename", "/first", "old.jpg", "new.jpg"),
            ("taxon_rename", "/second|library", "before.png", "after.png"),
        ] {
            connection
                .execute(
                    "INSERT INTO photo_operations (source, root_path, input_json) VALUES (?, ?, '[]')",
                    params![source, root_path],
                )
                .unwrap();
            let operation_id = connection.last_insert_rowid();
            connection
                .execute(
                    r#"
                    INSERT INTO photo_operation_items (
                        operation_id, row_number, photo_id,
                        directory_relative_path, old_filename, new_filename
                    ) VALUES (?, 1, ?, 'folder', ?, ?)
                    "#,
                    params![operation_id, operation_id * 10, old_filename, new_filename],
                )
                .unwrap();
        }
        drop(connection);

        let single = export_photo_operation_csv(&database, 2).unwrap();
        let (single_headers, single_rows) = read_csv(&single);
        assert_eq!(single_headers, PHOTO_OPERATION_EXPORT_COLUMNS[..]);
        assert_eq!(single_rows.len(), 1);
        assert_eq!(single_rows[0].get(0), Some("2"));
        assert_eq!(single_rows[0].get(3), Some("/second|library"));

        let all = export_all_photo_operations_csv(&database).unwrap();
        let (all_headers, all_rows) = read_csv(&all);
        assert_eq!(all_headers, PHOTO_OPERATION_EXPORT_COLUMNS[..]);
        assert_eq!(all_rows.len(), 2);
        assert_eq!(
            all_rows
                .iter()
                .map(|row| row.get(0).unwrap())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
    }

    fn read_csv(input: &str) -> (csv::StringRecord, Vec<csv::StringRecord>) {
        let mut reader = ReaderBuilder::new()
            .delimiter(b'|')
            .from_reader(input.as_bytes());
        let headers = reader.headers().unwrap().clone();
        let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
        (headers, rows)
    }
}
