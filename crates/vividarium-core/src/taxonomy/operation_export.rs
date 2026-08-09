use std::collections::BTreeSet;

use csv::WriterBuilder;
use rusqlite::params_from_iter;

use super::formatted::{TAXONOMY_INPUT_COLUMNS, TaxonInputRow, get_taxonomy_name_separator};
use crate::operations;
use crate::{CoreError, CoreResult, Database};

pub fn write_operation_audit<W: std::io::Write>(
    database: &Database,
    operation_id: i64,
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect_taxonomy_metadata_context()?,
        Some(&[operation_id]),
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn write_operations_audit<W: std::io::Write>(
    database: &Database,
    operation_ids: &[i64],
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect_taxonomy_metadata_context()?,
        Some(operation_ids),
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn write_all_operation_audit<W: std::io::Write>(
    database: &Database,
    writer: &mut W,
) -> CoreResult<()> {
    operations::write_operation_audit(
        &database.connect_taxonomy_metadata_context()?,
        None,
        crate::general::get_csv_delimiter_byte(database)?,
        writer,
    )
}

pub fn export_operation_input(database: &Database, operation_id: i64) -> CoreResult<String> {
    export_inputs(database, Some(&[operation_id]))
}

pub fn export_operations_input(database: &Database, operation_ids: &[i64]) -> CoreResult<String> {
    export_inputs(database, Some(operation_ids))
}

pub fn export_all_replayable_inputs(database: &Database) -> CoreResult<String> {
    export_inputs(database, None)
}

fn export_inputs(database: &Database, operation_ids: Option<&[i64]>) -> CoreResult<String> {
    let separator = get_taxonomy_name_separator(database)?;
    let delimiter = crate::general::get_csv_delimiter_byte(database)?;
    let connection = database.connect_taxonomy_metadata_context()?;
    if let Some(operation_ids) = operation_ids {
        validate_replayable_operations(&connection, operation_ids)?;
    }
    let mut writer = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    writer.write_record(TAXONOMY_INPUT_COLUMNS)?;
    let filter = operation_ids
        .map(|operation_ids| {
            let unique = operation_ids.iter().copied().collect::<BTreeSet<_>>();
            let placeholders = std::iter::repeat_n("?", unique.len())
                .collect::<Vec<_>>()
                .join(",");
            (
                format!("WHERE formatted.operation_id IN ({placeholders})"),
                unique.into_iter().collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            (
                "WHERE operations.has_formatted_input = 1".into(),
                Vec::new(),
            )
        });
    let mut statement = connection.prepare(&format!(
        r#"
        SELECT formatted.input_json
        FROM operation_formatted_inputs AS formatted
        JOIN operations USING (operation_id)
        {}
        ORDER BY formatted.operation_id, formatted.sequence
        "#,
        filter.0
    ))?;
    let mut rows = statement.query(params_from_iter(filter.1))?;
    while let Some(row) = rows.next()? {
        let input_json = row.get::<_, String>(0)?;
        let input: TaxonInputRow = serde_json::from_str(&input_json).map_err(|error| {
            CoreError::Consistency(format!("invalid formatted operation input: {error}"))
        })?;
        writer.write_record(input_record(input, &separator))?;
    }
    finish_csv(writer)
}

fn validate_replayable_operations(
    connection: &rusqlite::Connection,
    operation_ids: &[i64],
) -> CoreResult<()> {
    if operation_ids.is_empty() {
        return Err(CoreError::InvalidArgument(
            "at least one operation id is required".into(),
        ));
    }
    let operation_ids = operation_ids.iter().copied().collect::<BTreeSet<_>>();
    if operation_ids.iter().any(|value| *value <= 0) {
        return Err(CoreError::InvalidArgument(
            "operation ids must be positive".into(),
        ));
    }
    let placeholders = std::iter::repeat_n("?", operation_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let supported = connection.query_row(
        &format!(
            r#"
            SELECT COUNT(*)
            FROM operations
            WHERE operation_id IN ({placeholders})
              AND has_formatted_input = 1
            "#
        ),
        params_from_iter(operation_ids.iter()),
        |row| row.get::<_, usize>(0),
    )?;
    if supported != operation_ids.len() {
        return Err(CoreError::InvalidArgument(
            "every selected operation must have formatted input".into(),
        ));
    }
    Ok(())
}

fn input_record(row: TaxonInputRow, separator: &str) -> Vec<String> {
    vec![
        row.kingdom.unwrap_or_default(),
        row.order.unwrap_or_default(),
        row.family.unwrap_or_default(),
        row.genus.unwrap_or_default(),
        row.species.unwrap_or_default(),
        row.authority_year.unwrap_or_default(),
        row.synonyms.join(separator),
        row.zh_name.unwrap_or_default(),
        row.zh_alias.join(separator),
        row.en_name.unwrap_or_default(),
        row.en_alias.join(separator),
        row.geological_range.unwrap_or_default(),
        row.source.unwrap_or_default(),
    ]
}

fn finish_csv(writer: csv::Writer<Vec<u8>>) -> CoreResult<String> {
    let bytes = writer.into_inner().map_err(|error| error.into_error())?;
    String::from_utf8(bytes).map_err(|error| {
        CoreError::InvalidArgument(format!("invalid UTF-8 taxonomy operation export: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::super::formatted::{apply_rows, parse_taxonomy_input_csv};
    use super::*;

    #[test]
    fn exports_selected_or_all_replayable_inputs_with_one_header() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        crate::general::update_general_settings(
            &database,
            &crate::general::GeneralSettings {
                csv_delimiter: ";".into(),
                ..crate::general::GeneralSettings::default()
            },
        )
        .unwrap();
        let first = apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    synonyms: vec!["Metazoa".into()],
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    order: Some("Unmatched order".into()),
                    ..TaxonInputRow::default()
                },
            ],
        )
        .unwrap();
        let second = apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();

        let selected =
            export_operations_input(&database, &[first.operation_id, second.operation_id]).unwrap();
        assert_eq!(
            selected.lines().next().unwrap(),
            TAXONOMY_INPUT_COLUMNS.join(";")
        );
        let rows = parse_taxonomy_input_csv(&database, &selected).unwrap();
        assert_eq!(rows.len(), 3);

        let all = export_all_replayable_inputs(&database).unwrap();
        assert_eq!(
            all.lines()
                .filter(|line| *line == TAXONOMY_INPUT_COLUMNS.join(";"))
                .count(),
            1
        );
        assert_eq!(parse_taxonomy_input_csv(&database, &all).unwrap().len(), 3);
        let mut audit = Vec::new();
        write_operations_audit(
            &database,
            &[first.operation_id, second.operation_id],
            &mut audit,
        )
        .unwrap();
        assert!(
            String::from_utf8(audit)
                .unwrap()
                .starts_with("operation_id;sequence;")
        );
    }
}
