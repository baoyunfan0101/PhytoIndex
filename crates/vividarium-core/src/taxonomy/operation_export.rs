use csv::WriterBuilder;
use rusqlite::OptionalExtension;

use super::formatted::{TAXONOMY_INPUT_COLUMNS, TaxonInputRow, get_taxonomy_name_separator};
use crate::{CoreError, CoreResult, Database};

pub fn export_taxonomy_operation_csv(database: &Database, operation_id: i64) -> CoreResult<String> {
    validate_operation_id(operation_id)?;
    let connection = database.connect_taxonomy_context()?;
    let input_json = connection
        .query_row(
            "SELECT input_json FROM taxonomy_operations WHERE operation_id = ?",
            [operation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("taxonomy operation {operation_id}")))?;
    export_taxonomy_inputs(database, std::iter::once(input_json))
}

pub fn export_all_taxonomy_operations_csv(database: &Database) -> CoreResult<String> {
    let connection = database.connect_taxonomy_context()?;
    let mut statement =
        connection.prepare("SELECT input_json FROM taxonomy_operations ORDER BY operation_id")?;
    let input_json = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    export_taxonomy_inputs(database, input_json)
}

fn export_taxonomy_inputs(
    database: &Database,
    operation_inputs: impl IntoIterator<Item = String>,
) -> CoreResult<String> {
    let separator = get_taxonomy_name_separator(database)?;
    let mut writer = WriterBuilder::new().delimiter(b'|').from_writer(Vec::new());
    writer.write_record(TAXONOMY_INPUT_COLUMNS)?;
    for input_json in operation_inputs {
        let rows: Vec<TaxonInputRow> = serde_json::from_str(&input_json).map_err(|error| {
            CoreError::InvalidArgument(format!("invalid taxonomy operation input: {error}"))
        })?;
        for row in rows {
            writer.write_record(input_record(row, &separator))?;
        }
    }
    finish_csv(writer)
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

fn validate_operation_id(operation_id: i64) -> CoreResult<()> {
    if operation_id <= 0 {
        return Err(CoreError::InvalidArgument(
            "operation id must be positive".into(),
        ));
    }
    Ok(())
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
    fn exports_one_or_all_operation_inputs_as_formatted_update_csv() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
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
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();

        let single = export_taxonomy_operation_csv(&database, first.operation_id).unwrap();
        assert_eq!(
            single.lines().next().unwrap(),
            TAXONOMY_INPUT_COLUMNS.join("|")
        );
        let single_rows = parse_taxonomy_input_csv(&database, &single).unwrap();
        assert_eq!(single_rows.len(), 2);
        assert_eq!(single_rows[0].kingdom.as_deref(), Some("Animalia"));
        assert_eq!(single_rows[0].synonyms, ["Metazoa"]);
        assert_eq!(single_rows[1].order.as_deref(), Some("Unmatched order"));

        let all = export_all_taxonomy_operations_csv(&database).unwrap();
        assert_eq!(
            all.lines()
                .filter(|line| *line == TAXONOMY_INPUT_COLUMNS.join("|"))
                .count(),
            1
        );
        let all_rows = parse_taxonomy_input_csv(&database, &all).unwrap();
        assert_eq!(all_rows.len(), 3);
        assert_eq!(all_rows[0].kingdom.as_deref(), Some("Animalia"));
        assert_eq!(all_rows[1].order.as_deref(), Some("Unmatched order"));
        assert_eq!(all_rows[2].order.as_deref(), Some("Carnivora"));
    }
}
