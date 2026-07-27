use std::collections::{BTreeMap, HashMap};

use rusqlite::types::Value as SqlValue;
use rusqlite::{Transaction, params, params_from_iter};
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{CoreError, CoreResult};
use crate::models::Photo;
use crate::naming::PhotoFilenameParser;
use crate::taxonomy::{
    TaxonDisplayNames, TaxonRank, TaxonSummary, TaxonomyNameType, load_taxon_summaries,
};

mod actions;
mod candidates;
mod name_match;
mod navigation;
mod status;
mod tree;

pub use actions::{
    clear_photo_mapping, get_metadata, get_photo_mapping, get_photo_mapping_candidates,
    remap_photo, set_photo_mapping,
};
pub use name_match::{
    PhotoNameField, PhotoNameMatchSettings, get_photo_name_match_settings,
    set_photo_name_match_settings,
};
pub use navigation::{list_taxon_photos, search_photo_taxa, suggest_photo_taxa};
pub use status::{list_photos_by_mapping_status, search_photos_by_mapping_status};
pub use tree::{browse_photo_taxon, get_photo_taxon_node};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoTaxonStatus {
    Matched,
    Unmatched,
    Ambiguous,
    Processing,
}

impl PhotoTaxonStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Unmatched => "unmatched",
            Self::Ambiguous => "ambiguous",
            Self::Processing => "processing",
        }
    }

    fn from_str(value: &str) -> CoreResult<Self> {
        match value {
            "matched" => Ok(Self::Matched),
            "unmatched" => Ok(Self::Unmatched),
            "ambiguous" => Ok(Self::Ambiguous),
            "processing" => Ok(Self::Processing),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid photo taxon status: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoMappingSummary {
    pub photo_id: i64,
    pub taxon_id: Option<i64>,
    pub status: PhotoTaxonStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoMatchedName {
    pub name_id: i64,
    pub name_type: TaxonomyNameType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoTaxonCandidate {
    pub summary: TaxonSummary,
    pub matched_names: Vec<PhotoMatchedName>,
    pub accepted_names: TaxonDisplayNames,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoTaxonUsage {
    pub taxon_id: i64,
    pub rank: TaxonRank,
    pub names: TaxonDisplayNames,
    pub direct_photo_count: i64,
    pub subtree_photo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoTaxonNode {
    pub taxon: Option<PhotoTaxonUsage>,
    pub subtree_photo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhotoTaxonItem {
    Taxon { taxon: PhotoTaxonUsage },
    Photo { photo: Photo },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhotoMappingListStatus {
    Matched,
    Unmatched,
    Ambiguous,
    Processing,
}

impl PhotoMappingListStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::Unmatched => "unmatched",
            Self::Ambiguous => "ambiguous",
            Self::Processing => "processing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoMappingListItem {
    pub photo: Photo,
    pub mapping: PhotoMappingSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhotoMappingRunResult {
    pub processed: usize,
    pub changed: usize,
    pub pending: i64,
}

const PHOTO_TAXON_CANDIDATE_LIMIT: usize = 500;
const PHOTO_MAPPING_BATCH_SIZE: usize = 200;

pub type MappingProgressCallback<'a> = dyn FnMut(u64, Option<u64>, &str) + Send + 'a;

pub(crate) fn queue_photo_ids(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
    reason: &str,
) -> CoreResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }
    let mut statement = transaction.prepare_cached(
        r#"
        INSERT INTO photo_mapping_queue (photo_id, reason)
        VALUES (?, ?)
        ON CONFLICT(photo_id) DO UPDATE SET reason = excluded.reason
        "#,
    )?;
    for photo_id in photo_ids {
        statement.execute(params![photo_id, reason])?;
    }
    Ok(())
}

pub fn process_pending_photo_matches(
    database: &Database,
    progress: &mut MappingProgressCallback<'_>,
) -> CoreResult<PhotoMappingRunResult> {
    let connection = database.connect()?;
    let filename_parser = PhotoFilenameParser::load(&connection)?;
    let match_settings = name_match::load(&connection)?;
    let total = connection.query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
        row.get::<_, i64>(0)
    })?;
    drop(connection);

    let mut processed = 0usize;
    let mut changed = 0usize;
    progress(0, Some(total as u64), "Matching photo names");
    loop {
        let mut connection = database.connect()?;
        let transaction = connection.transaction()?;
        let photo_ids = {
            let mut statement = transaction.prepare(
                r#"
                SELECT photo_id
                FROM photo_mapping_queue
                ORDER BY photo_id
                LIMIT ?
                "#,
            )?;
            statement
                .query_map([PHOTO_MAPPING_BATCH_SIZE as i64], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if photo_ids.is_empty() {
            break;
        }
        changed +=
            remap_photo_ids_with(&transaction, &photo_ids, &filename_parser, &match_settings)?;
        delete_queued_photo_ids(&transaction, &photo_ids)?;
        transaction.commit()?;
        processed += photo_ids.len();
        progress(processed as u64, Some(total as u64), "Matching photo names");
    }
    let connection = database.connect()?;
    let queued = connection.query_row("SELECT COUNT(*) FROM photo_mapping_queue", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(PhotoMappingRunResult {
        processed,
        changed,
        pending: queued,
    })
}

pub(crate) fn remap_photo_ids(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
) -> CoreResult<usize> {
    if photo_ids.is_empty() {
        return Ok(0);
    }
    let filename_parser = PhotoFilenameParser::load(transaction)?;
    let match_settings = name_match::load(transaction)?;
    remap_photo_ids_with(transaction, photo_ids, &filename_parser, &match_settings)
}

fn remap_photo_ids_with(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
    filename_parser: &PhotoFilenameParser,
    match_settings: &PhotoNameMatchSettings,
) -> CoreResult<usize> {
    if photo_ids.is_empty() {
        return Ok(0);
    }
    let photos = load_photo_names(transaction, photo_ids)?;
    let old_mappings = load_mappings(transaction, photo_ids)?;
    let mut direct_deltas = BTreeMap::<i64, i64>::new();
    let mut changed = 0usize;
    for (photo_id, filename) in photos {
        let results =
            match_photo_taxa_with(transaction, filename_parser, match_settings, &filename)?;
        let old_mapping = old_mappings.get(&photo_id).copied();
        let old_taxon_id = old_mapping.and_then(|(taxon_id, status)| {
            (status == PhotoTaxonStatus::Matched)
                .then_some(taxon_id)
                .flatten()
        });
        let (new_taxon_id, new_status) = if old_taxon_id.is_some_and(|taxon_id| {
            results
                .iter()
                .any(|result| result.summary.taxon_id == taxon_id)
        }) {
            (old_taxon_id, PhotoTaxonStatus::Matched)
        } else {
            (resolved_taxon_id(&results), resolved_status(&results))
        };
        if old_taxon_id != new_taxon_id {
            if let Some(taxon_id) = old_taxon_id {
                *direct_deltas.entry(taxon_id).or_default() -= 1;
            }
            if let Some(taxon_id) = new_taxon_id {
                *direct_deltas.entry(taxon_id).or_default() += 1;
            }
        }
        if old_mapping != Some((new_taxon_id, new_status)) {
            transaction.execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (?, ?, ?)
                ON CONFLICT(photo_id) DO UPDATE SET
                    taxon_id = excluded.taxon_id,
                    status = excluded.status
                "#,
                params![photo_id, new_taxon_id, new_status.as_str()],
            )?;
            changed += 1;
        }
        if new_status == PhotoTaxonStatus::Ambiguous {
            candidates::replace(transaction, photo_id, &results)?;
        } else {
            candidates::clear(transaction, photo_id)?;
        }
    }
    apply_usage_deltas(transaction, &direct_deltas)?;
    Ok(changed)
}

pub(crate) fn remove_photo_mappings(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
) -> CoreResult<()> {
    if photo_ids.is_empty() {
        return Ok(());
    }
    let old_taxa = load_mapped_taxa(transaction, photo_ids)?;
    let mut direct_deltas = BTreeMap::<i64, i64>::new();
    for taxon_id in old_taxa.into_values() {
        *direct_deltas.entry(taxon_id).or_default() -= 1;
    }
    let selection = id_selection(transaction, photo_ids, "photo_id", "temp_mapping_photo_ids")?;
    transaction.execute(
        &format!(
            "DELETE FROM photo_taxon_mapping WHERE {}",
            selection.predicate
        ),
        params_from_iter(selection.values),
    )?;
    apply_usage_deltas(transaction, &direct_deltas)
}

pub(crate) fn remove_directory_mappings(
    transaction: &Transaction<'_>,
    directory_ids: &[i64],
) -> CoreResult<()> {
    if directory_ids.is_empty() {
        return Ok(());
    }
    let selection = id_selection(
        transaction,
        directory_ids,
        "directory_id",
        "temp_mapping_directory_ids",
    )?;
    let mut statement = transaction.prepare(&format!(
        r#"
        WITH RECURSIVE descendants(directory_id) AS (
            SELECT directory_id FROM photo_directories WHERE {}
            UNION ALL
            SELECT child.directory_id
            FROM photo_directories AS child
            JOIN descendants ON child.parent_directory_id = descendants.directory_id
        )
        SELECT photos.photo_id
        FROM photos
        JOIN descendants USING (directory_id)
        "#,
        selection.predicate
    ))?;
    let rows = statement.query_map(params_from_iter(selection.values), |row| {
        row.get::<_, i64>(0)
    })?;
    let photo_ids = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    remove_photo_mappings(transaction, &photo_ids)
}

fn match_photo_taxa_with(
    connection: &rusqlite::Connection,
    filename_parser: &PhotoFilenameParser,
    settings: &PhotoNameMatchSettings,
    filename: &str,
) -> CoreResult<Vec<PhotoTaxonCandidate>> {
    let parsed = filename_parser.parse(filename)?;
    for field in &settings.priority {
        let Some(name) = field.value(&parsed.info) else {
            continue;
        };
        let candidates = find_photo_name_candidates(connection, *field, name)?;
        if !candidates.is_empty() {
            return Ok(candidates);
        }
    }
    Ok(Vec::new())
}

fn find_photo_name_candidates(
    connection: &rusqlite::Connection,
    field: PhotoNameField,
    name: &str,
) -> CoreResult<Vec<PhotoTaxonCandidate>> {
    let [first_name_type, second_name_type] = field.name_types();
    let mut statement = connection.prepare(
        r#"
        WITH candidate_taxa AS (
            SELECT DISTINCT taxa.taxon_id
            FROM taxa
            JOIN taxon_names USING (taxon_id)
            WHERE taxa.rank = ?
              AND taxon_names.name_type IN (?, ?)
              AND taxon_names.normalized_name = lower(?)
            ORDER BY taxa.taxon_id
            LIMIT ?
        )
        SELECT candidate_taxa.taxon_id, taxon_names.name_id,
               taxon_names.name_type, taxon_names.name
        FROM candidate_taxa
        JOIN taxon_names USING (taxon_id)
        WHERE taxon_names.name_type IN (?, ?)
          AND taxon_names.normalized_name = lower(?)
        ORDER BY candidate_taxa.taxon_id, taxon_names.name_type,
                 taxon_names.name_id
        "#,
    )?;
    let rows = statement
        .query_map(
            params![
                field.rank().code(),
                first_name_type.code(),
                second_name_type.code(),
                name,
                PHOTO_TAXON_CANDIDATE_LIMIT as i64,
                first_name_type.code(),
                second_name_type.code(),
                name
            ],
            |row| {
                let name_type_code = row.get::<_, i64>(2)?;
                let name_type = TaxonomyNameType::from_code(name_type_code).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Integer,
                        Box::new(error),
                    )
                })?;
                Ok((
                    row.get::<_, i64>(0)?,
                    PhotoMatchedName {
                        name_id: row.get(1)?,
                        name_type,
                        name: row.get(3)?,
                    },
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut matched_names_by_taxon = BTreeMap::<i64, Vec<PhotoMatchedName>>::new();
    for (taxon_id, matched_name) in rows {
        matched_names_by_taxon
            .entry(taxon_id)
            .or_default()
            .push(matched_name);
    }
    let taxon_ids = matched_names_by_taxon.keys().copied().collect::<Vec<_>>();
    let summaries = load_taxon_summaries(connection, &taxon_ids)?;
    Ok(summaries
        .into_iter()
        .map(|summary| PhotoTaxonCandidate {
            accepted_names: summary.names.clone(),
            matched_names: matched_names_by_taxon
                .remove(&summary.taxon_id)
                .unwrap_or_default(),
            summary,
        })
        .collect())
}

fn resolved_taxon_id(results: &[PhotoTaxonCandidate]) -> Option<i64> {
    match results {
        [candidate] => Some(candidate.summary.taxon_id),
        _ => None,
    }
}

fn resolved_status(results: &[PhotoTaxonCandidate]) -> PhotoTaxonStatus {
    match results.len() {
        0 => PhotoTaxonStatus::Unmatched,
        1 => PhotoTaxonStatus::Matched,
        _ => PhotoTaxonStatus::Ambiguous,
    }
}

fn load_photo_names(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
) -> CoreResult<Vec<(i64, String)>> {
    let selection = id_selection(transaction, photo_ids, "photo_id", "temp_mapping_photo_ids")?;
    let mut statement = transaction.prepare(&format!(
        "SELECT photo_id, filename FROM photos WHERE {} ORDER BY photo_id",
        selection.predicate
    ))?;
    let rows = statement.query_map(params_from_iter(selection.values), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_mapped_taxa(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
) -> CoreResult<HashMap<i64, i64>> {
    let selection = id_selection(transaction, photo_ids, "photo_id", "temp_mapping_photo_ids")?;
    let mut statement = transaction.prepare(&format!(
        r#"
        SELECT photo_id, taxon_id
        FROM photo_taxon_mapping
        WHERE status = 'matched' AND {}
        "#,
        selection.predicate
    ))?;
    let rows = statement.query_map(params_from_iter(selection.values), |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

fn load_mappings(
    transaction: &Transaction<'_>,
    photo_ids: &[i64],
) -> CoreResult<HashMap<i64, (Option<i64>, PhotoTaxonStatus)>> {
    let selection = id_selection(transaction, photo_ids, "photo_id", "temp_mapping_photo_ids")?;
    let mut statement = transaction.prepare(&format!(
        r#"
        SELECT photo_id, taxon_id, status
        FROM photo_taxon_mapping
        WHERE {}
        "#,
        selection.predicate
    ))?;
    let rows = statement.query_map(params_from_iter(selection.values), |row| {
        let status = row.get::<_, String>(2)?;
        let status = PhotoTaxonStatus::from_str(&status).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        Ok((row.get::<_, i64>(0)?, (row.get(1)?, status)))
    })?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

fn apply_usage_deltas(
    transaction: &Transaction<'_>,
    direct_deltas: &BTreeMap<i64, i64>,
) -> CoreResult<()> {
    transaction.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS temp_photo_taxon_deltas (
            taxon_id INTEGER PRIMARY KEY,
            delta INTEGER NOT NULL
        ) WITHOUT ROWID;
        DELETE FROM temp_photo_taxon_deltas;
        "#,
    )?;
    {
        let mut statement = transaction.prepare_cached(
            "INSERT INTO temp_photo_taxon_deltas (taxon_id, delta) VALUES (?, ?)",
        )?;
        for (&taxon_id, &delta) in direct_deltas {
            if delta != 0 {
                statement.execute(params![taxon_id, delta])?;
            }
        }
    }
    transaction.execute_batch(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS temp_photo_usage_deltas (
            taxon_id INTEGER PRIMARY KEY,
            direct_delta INTEGER NOT NULL,
            subtree_delta INTEGER NOT NULL
        ) WITHOUT ROWID;
        DELETE FROM temp_photo_usage_deltas;

        WITH RECURSIVE lineage(taxon_id, parent_taxon_id, delta) AS (
            SELECT taxa.taxon_id, taxa.parent_taxon_id, seeds.delta
            FROM temp_photo_taxon_deltas AS seeds
            JOIN taxa ON taxa.taxon_id = seeds.taxon_id
            UNION ALL
            SELECT parent.taxon_id, parent.parent_taxon_id, child.delta
            FROM lineage AS child
            JOIN taxa AS parent ON parent.taxon_id = child.parent_taxon_id
        ),
        subtree_deltas AS (
            SELECT taxon_id, SUM(delta) AS delta
            FROM lineage
            GROUP BY taxon_id
        ),
        affected_taxa AS (
            SELECT taxon_id FROM temp_photo_taxon_deltas
            UNION
            SELECT taxon_id FROM subtree_deltas
        )
        INSERT INTO temp_photo_usage_deltas (
            taxon_id, direct_delta, subtree_delta
        )
        SELECT affected_taxa.taxon_id,
               COALESCE(direct.delta, 0),
               COALESCE(subtree.delta, 0)
        FROM affected_taxa
        LEFT JOIN temp_photo_taxon_deltas AS direct USING (taxon_id)
        LEFT JOIN subtree_deltas AS subtree USING (taxon_id)
        WHERE TRUE;

        UPDATE photo_taxon_usage
        SET direct_photo_count = direct_photo_count + (
                SELECT direct_delta
                FROM temp_photo_usage_deltas AS delta
                WHERE delta.taxon_id = photo_taxon_usage.taxon_id
            ),
            subtree_photo_count = subtree_photo_count + (
                SELECT subtree_delta
                FROM temp_photo_usage_deltas AS delta
                WHERE delta.taxon_id = photo_taxon_usage.taxon_id
            )
        WHERE taxon_id IN (SELECT taxon_id FROM temp_photo_usage_deltas);

        INSERT INTO photo_taxon_usage (
            taxon_id, direct_photo_count, subtree_photo_count
        )
        SELECT delta.taxon_id, delta.direct_delta, delta.subtree_delta
        FROM temp_photo_usage_deltas AS delta
        LEFT JOIN photo_taxon_usage AS usage USING (taxon_id)
        WHERE usage.taxon_id IS NULL;
        "#,
    )?;
    transaction.execute(
        "DELETE FROM photo_taxon_usage WHERE direct_photo_count = 0 AND subtree_photo_count = 0",
        [],
    )?;
    Ok(())
}

fn mapping_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoMappingSummary> {
    let status = row.get::<_, String>(2)?;
    Ok(PhotoMappingSummary {
        photo_id: row.get(0)?,
        taxon_id: row.get(1)?,
        status: PhotoTaxonStatus::from_str(&status).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}

struct IdSelection {
    predicate: String,
    values: Vec<SqlValue>,
}

fn id_selection(
    transaction: &Transaction<'_>,
    ids: &[i64],
    column: &str,
    temp_table: &str,
) -> CoreResult<IdSelection> {
    const INLINE_ID_LIMIT: usize = 500;
    if ids.len() <= INLINE_ID_LIMIT {
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        return Ok(IdSelection {
            predicate: format!("{column} IN ({placeholders})"),
            values: ids.iter().copied().map(SqlValue::Integer).collect(),
        });
    }
    transaction.execute_batch(&format!(
        r#"
        CREATE TEMP TABLE IF NOT EXISTS {temp_table} (
            value INTEGER PRIMARY KEY
        ) WITHOUT ROWID;
        DELETE FROM {temp_table};
        "#
    ))?;
    let mut statement =
        transaction.prepare_cached(&format!("INSERT INTO {temp_table} (value) VALUES (?)"))?;
    for id in ids {
        statement.execute([id])?;
    }
    Ok(IdSelection {
        predicate: format!("{column} IN (SELECT value FROM {temp_table})"),
        values: Vec::new(),
    })
}

fn delete_queued_photo_ids(transaction: &Transaction<'_>, photo_ids: &[i64]) -> CoreResult<()> {
    let selection = id_selection(transaction, photo_ids, "photo_id", "temp_mapping_photo_ids")?;
    transaction.execute(
        &format!(
            "DELETE FROM photo_mapping_queue WHERE {}",
            selection.predicate
        ),
        params_from_iter(selection.values),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{NamingHookKind, set_naming_hook, take_hook_compile_count};
    use crate::photos::{self, open_library, refresh_directory};
    use crate::taxonomy::{
        TaxonInputRow, TaxonUpdateInput, apply_rows, execute_custom_taxonomy_sql, update_taxon,
    };
    use std::fs;

    fn insert_test_photo(
        connection: &rusqlite::Connection,
        directory_id: i64,
        filename: &str,
    ) -> i64 {
        connection
            .execute(
                r#"
                INSERT INTO photos (
                    directory_id, filename, file_size, modified_at_ns
                ) VALUES (?, ?, 1, 1)
                "#,
                params![directory_id, filename],
            )
            .unwrap();
        connection.last_insert_rowid()
    }

    #[test]
    fn one_mapping_run_compiles_the_hook_once_across_batches() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        for index in 0..PHOTO_MAPPING_BATCH_SIZE + 1 {
            let photo_id = insert_test_photo(
                &connection,
                library.root_directory_id,
                &format!("Unknown {index}.jpg"),
            );
            connection
                .execute(
                    "INSERT INTO photo_mapping_queue (photo_id, reason) VALUES (?, 'refresh')",
                    [photo_id],
                )
                .unwrap();
        }
        drop(connection);

        take_hook_compile_count();
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        let result = process_pending_photo_matches(&database, &mut progress).unwrap();

        assert_eq!(result.processed, PHOTO_MAPPING_BATCH_SIZE + 1);
        assert_eq!(take_hook_compile_count(), 1);
    }

    #[test]
    fn six_dimension_priority_controls_photo_mapping() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("input.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    ..Default::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    ..Default::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    ..Default::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    ..Default::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    species: Some("Canis lupus".into()),
                    zh_name: Some("wolf".into()),
                    ..Default::default()
                },
            ],
        )
        .unwrap();
        set_naming_hook(
            &database,
            NamingHookKind::PhotoFilename,
            Some(
                r#"
                fn parse_photo_filename(filename) {
                    #{
                        info: #{
                            family_sci: "Canidae",
                            species_zh: "wolf"
                        },
                        suffix: ".jpg"
                    }
                }
                "#,
            ),
        )
        .unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let species_mapping = get_photo_mapping(&database, photo.photo_id).unwrap();
        assert_eq!(species_mapping.status, PhotoTaxonStatus::Matched);
        assert!(
            get_photo_mapping_candidates(&database, photo.photo_id)
                .unwrap()
                .is_empty()
        );
        let species_summary =
            crate::taxonomy::get_taxon_summary(&database, species_mapping.taxon_id.unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(species_summary.names.zh_name.as_deref(), Some("wolf"));

        set_photo_name_match_settings(
            &database,
            &PhotoNameMatchSettings {
                priority: vec![
                    PhotoNameField::FamilySci,
                    PhotoNameField::SpeciesSci,
                    PhotoNameField::SpeciesZh,
                    PhotoNameField::GenusSci,
                    PhotoNameField::GenusZh,
                    PhotoNameField::FamilyZh,
                ],
            },
        )
        .unwrap();
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let family_mapping = get_photo_mapping(&database, photo.photo_id).unwrap();
        assert_eq!(family_mapping.status, PhotoTaxonStatus::Matched);
        assert!(
            get_photo_mapping_candidates(&database, photo.photo_id)
                .unwrap()
                .is_empty()
        );
        let family_summary =
            crate::taxonomy::get_taxon_summary(&database, family_mapping.taxon_id.unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(family_summary.rank, TaxonRank::Family);
    }

    #[test]
    fn matches_the_filename_stem_and_builds_sparse_usage() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Canis lupus.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let rows = [
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                ..Default::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                ..Default::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                ..Default::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                genus: Some("Canis".into()),
                ..Default::default()
            },
            TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                family: Some("Canidae".into()),
                genus: Some("Canis".into()),
                species: Some("Canis lupus".into()),
                ..Default::default()
            },
        ];
        apply_rows(&database, &rows).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        let mapping = get_photo_mapping(&database, photo.photo_id).unwrap();
        assert_eq!(mapping.status, PhotoTaxonStatus::Matched);
        assert!(
            get_photo_mapping_candidates(&database, photo.photo_id)
                .unwrap()
                .is_empty()
        );
        let species_id = mapping.taxon_id.unwrap();
        let species_summary = crate::taxonomy::get_taxon_summary(&database, species_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            species_summary.names.sci_name.as_deref(),
            Some("Canis lupus")
        );
        assert_eq!(mapping.taxon_id, Some(species_id));
        let node = get_photo_taxon_node(&database, mapping.taxon_id, false).unwrap();
        assert_eq!(node.taxon.as_ref().unwrap().direct_photo_count, 1);
        assert_eq!(node.subtree_photo_count, 1);
        let sparse_root = get_photo_taxon_node(&database, None, false).unwrap();
        assert_eq!(sparse_root.subtree_photo_count, 1);
        let root_page = browse_photo_taxon(&database, None, false, true, None, 20).unwrap();
        assert!(matches!(root_page.items[0], PhotoTaxonItem::Taxon { .. }));
        let page = browse_photo_taxon(&database, mapping.taxon_id, false, true, None, 20).unwrap();
        assert_eq!(
            page.items,
            vec![PhotoTaxonItem::Photo {
                photo: photo.clone()
            }]
        );
        assert_eq!(page.next_cursor, None);
        execute_custom_taxonomy_sql(
            &database,
            "UPDATE taxon_names SET name = 'Canis lycaon' WHERE name = 'Canis lupus'",
            None,
        )
        .unwrap();
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let old_taxon_id = mapping.taxon_id;
        let mapping = get_photo_mapping(&database, mapping.photo_id).unwrap();
        assert_eq!(mapping.status, PhotoTaxonStatus::Matched);
        assert_ne!(mapping.taxon_id, old_taxon_id);
        assert!(get_photo_taxon_node(&database, old_taxon_id, false).is_err());
    }

    #[test]
    fn persists_ambiguous_candidates_and_accepts_a_forced_mapping() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Shared name.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        for accepted_name in ["Shared name", "Different name"] {
            connection
                .execute("INSERT INTO taxa (rank) VALUES (5)", [])
                .unwrap();
            let taxon_id = connection.last_insert_rowid();
            connection
                .execute(
                    r#"
                    INSERT INTO taxon_names (taxon_id, name_type, name)
                    VALUES (?, 1, ?)
                    "#,
                    params![taxon_id, accepted_name],
                )
                .unwrap();
            connection
                .execute(
                    r#"
                    INSERT INTO taxon_names (taxon_id, name_type, name)
                    VALUES (?, 2, 'Shared name')
                    "#,
                    [taxon_id],
                )
                .unwrap();
        }
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        assert_eq!(
            get_photo_mapping(&database, photo.photo_id).unwrap().status,
            PhotoTaxonStatus::Processing
        );
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let mapping = get_photo_mapping(&database, photo.photo_id).unwrap();
        let candidates = get_photo_mapping_candidates(&database, photo.photo_id).unwrap();
        assert_eq!(mapping.status, PhotoTaxonStatus::Ambiguous);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].matched_names.len(), 2);
        assert_eq!(candidates[1].matched_names.len(), 1);
        let connection = database.connect().unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM photo_taxon_candidates WHERE photo_id = ?",
                    [photo.photo_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM photo_taxon_candidate_names WHERE photo_id = ?",
                    [photo.photo_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            3
        );
        drop(connection);
        let selected_taxon_id = candidates[0].summary.taxon_id;
        let mut taxonomy = database.connect_taxonomy().unwrap();
        let transaction = taxonomy.transaction().unwrap();
        crate::taxonomy::sync::record_event(&transaction, None, [selected_taxon_id], false)
            .unwrap();
        transaction.commit().unwrap();
        crate::taxonomy::sync::synchronize_all_photo_libraries(&database).unwrap();
        let processing = get_photo_mapping(&database, photo.photo_id).unwrap();
        assert_eq!(processing.status, PhotoTaxonStatus::Processing);
        assert_eq!(processing.taxon_id, None);
        assert!(
            get_photo_mapping_candidates(&database, photo.photo_id)
                .unwrap()
                .is_empty()
        );
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let mapping = get_photo_mapping(&database, photo.photo_id).unwrap();
        let candidates = get_photo_mapping_candidates(&database, photo.photo_id).unwrap();
        assert_eq!(mapping.status, PhotoTaxonStatus::Ambiguous);
        assert_eq!(candidates.len(), 2);
        let selected = set_photo_mapping(&database, photo.photo_id, selected_taxon_id).unwrap();
        assert_eq!(selected.status, PhotoTaxonStatus::Matched);
        assert_eq!(selected.taxon_id, Some(selected_taxon_id));
        assert_eq!(
            database
                .connect()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM photo_taxon_candidates WHERE photo_id = ?",
                    [photo.photo_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        let error = set_photo_mapping(&database, photo.photo_id, i64::MAX).unwrap_err();
        assert!(error.to_string().contains("taxon"));
    }

    #[test]
    fn clears_forces_and_automatically_recomputes_one_mapping() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Canis lupus.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO taxa (taxon_id, rank) VALUES (1, 5), (2, 5);
                INSERT INTO taxon_names (taxon_id, name_type, name) VALUES
                    (1, 1, 'Canis lupus'),
                    (2, 1, 'Forced taxon');
                "#,
            )
            .unwrap();
        drop(connection);
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        assert_eq!(
            get_photo_mapping(&database, photo.photo_id)
                .unwrap()
                .taxon_id,
            Some(1)
        );

        let forced = set_photo_mapping(&database, photo.photo_id, 2).unwrap();
        assert_eq!(forced.status, PhotoTaxonStatus::Matched);
        assert_eq!(forced.taxon_id, Some(2));
        assert!(get_photo_taxon_node(&database, Some(1), false).is_err());
        assert_eq!(
            get_photo_taxon_node(&database, Some(2), false)
                .unwrap()
                .subtree_photo_count,
            1
        );

        let cleared = clear_photo_mapping(&database, photo.photo_id).unwrap();
        assert_eq!(cleared.status, PhotoTaxonStatus::Unmatched);
        assert_eq!(cleared.taxon_id, None);
        assert!(get_photo_taxon_node(&database, Some(2), false).is_err());

        let remapped = remap_photo(&database, photo.photo_id).unwrap();
        assert_eq!(remapped.status, PhotoTaxonStatus::Matched);
        assert_eq!(remapped.taxon_id, Some(1));
        assert!(
            get_photo_mapping_candidates(&database, photo.photo_id)
                .unwrap()
                .is_empty()
        );
        assert!(set_photo_mapping(&database, photo.photo_id, i64::MAX).is_err());
        assert!(clear_photo_mapping(&database, i64::MAX).is_err());
        assert!(remap_photo(&database, i64::MAX).is_err());
    }

    #[test]
    fn does_not_synthesize_processing_for_a_missing_photo() {
        let data = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();

        assert!(matches!(
            get_photo_mapping(&database, 404).unwrap_err(),
            CoreError::NotFound(_)
        ));
    }

    #[test]
    fn rejects_a_photo_without_mapping_state() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Canis lupus.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        database
            .connect()
            .unwrap()
            .execute(
                "DELETE FROM photo_mapping_queue WHERE photo_id = ?",
                [photo.photo_id],
            )
            .unwrap();

        let mapping_error = get_photo_mapping(&database, photo.photo_id).unwrap_err();
        assert!(matches!(mapping_error, CoreError::Consistency(_)));
        assert!(
            mapping_error
                .to_string()
                .contains("neither a mapping nor a mapping queue entry")
        );

        let candidates_error = get_photo_mapping_candidates(&database, photo.photo_id).unwrap_err();
        assert!(matches!(candidates_error, CoreError::Consistency(_)));
    }

    #[test]
    fn queues_a_photo_when_its_selected_taxon_is_deleted() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Felis catus.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (4)", [])
            .unwrap();
        let parent_taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO taxa (parent_taxon_id, rank) VALUES (?, 5)",
                [parent_taxon_id],
            )
            .unwrap();
        let taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Felis catus')
                "#,
                [taxon_id],
            )
            .unwrap();
        drop(connection);
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let photo = photos::list_photos(&database).unwrap().remove(0);
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        set_photo_mapping(&database, photo.photo_id, taxon_id).unwrap();
        assert_eq!(
            get_photo_taxon_node(&database, Some(parent_taxon_id), false)
                .unwrap()
                .subtree_photo_count,
            1
        );

        crate::taxonomy::delete_taxon(&database, taxon_id).unwrap();

        let connection = database.connect().unwrap();
        let stored_mapping_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM photo_taxon_mapping WHERE photo_id = ?",
                [photo.photo_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_mapping_count, 0);
        drop(connection);
        assert!(get_photo_taxon_node(&database, Some(parent_taxon_id), false).is_err());
        assert_eq!(
            get_photo_taxon_node(&database, Some(parent_taxon_id), true)
                .unwrap()
                .subtree_photo_count,
            0
        );
        assert_eq!(
            get_photo_mapping(&database, photo.photo_id).unwrap().status,
            PhotoTaxonStatus::Processing
        );
        process_pending_photo_matches(&database, &mut progress).unwrap();
        assert_eq!(
            get_photo_mapping(&database, photo.photo_id).unwrap().status,
            PhotoTaxonStatus::Unmatched
        );
    }

    #[test]
    fn taxonomy_update_queues_only_affected_photos() {
        let data = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("Canis lupus.jpg"), b"photo").unwrap();
        fs::write(root.path().join("Felis catus.jpg"), b"photo").unwrap();
        fs::write(root.path().join("domestic cat.jpg"), b"photo").unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (1)", [])
            .unwrap();
        let canis_taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Canis lupus')
                "#,
                [canis_taxon_id],
            )
            .unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (1)", [])
            .unwrap();
        let felis_taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Felis catus')
                "#,
                [felis_taxon_id],
            )
            .unwrap();
        drop(connection);
        let library = open_library(&database, root.path().to_str().unwrap()).unwrap();
        refresh_directory(&database, library.root_directory_id).unwrap();
        let mut progress = |_: u64, _: Option<u64>, _: &str| {};
        process_pending_photo_matches(&database, &mut progress).unwrap();
        let photos = photos::list_photos(&database).unwrap();
        let canis_photo = photos
            .iter()
            .find(|photo| photo.filename == "Canis lupus.jpg")
            .unwrap();
        let felis_photo = photos
            .iter()
            .find(|photo| photo.filename == "Felis catus.jpg")
            .unwrap();
        let domestic_cat_photo = photos
            .iter()
            .find(|photo| photo.filename == "domestic cat.jpg")
            .unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (?, ?, 'matched')
                ON CONFLICT(photo_id) DO UPDATE
                SET taxon_id = excluded.taxon_id, status = excluded.status
                "#,
                params![canis_photo.photo_id, canis_taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (?, ?, 'matched')
                ON CONFLICT(photo_id) DO UPDATE
                SET taxon_id = excluded.taxon_id, status = excluded.status
                "#,
                params![felis_photo.photo_id, felis_taxon_id],
            )
            .unwrap();
        connection
            .execute("DELETE FROM photo_mapping_queue", [])
            .unwrap();
        drop(connection);

        update_taxon(
            &database,
            TaxonUpdateInput {
                taxon_id: felis_taxon_id,
                geological_range: None,
                en_name: Some("domestic cat".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            get_photo_mapping(&database, felis_photo.photo_id)
                .unwrap()
                .status,
            PhotoTaxonStatus::Processing
        );
        assert_eq!(
            get_photo_mapping(&database, canis_photo.photo_id)
                .unwrap()
                .status,
            PhotoTaxonStatus::Matched
        );
        assert_eq!(
            get_photo_mapping(&database, domestic_cat_photo.photo_id)
                .unwrap()
                .status,
            PhotoTaxonStatus::Unmatched
        );
        assert_eq!(get_metadata(&database).unwrap().processing_photo_count, 1);
    }

    #[test]
    fn taxon_browse_cursor_spans_children_and_photos() {
        let data = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (1)", [])
            .unwrap();
        let parent_taxon_id = connection.last_insert_rowid();
        connection
            .execute(
                r#"
                INSERT INTO taxon_names (taxon_id, name_type, name)
                VALUES (?, 1, 'Parent')
                "#,
                [parent_taxon_id],
            )
            .unwrap();
        let mut child_taxon_ids = Vec::new();
        for name in ["First child", "Second child"] {
            connection
                .execute(
                    "INSERT INTO taxa (parent_taxon_id, rank) VALUES (?, 2)",
                    [parent_taxon_id],
                )
                .unwrap();
            let child_taxon_id = connection.last_insert_rowid();
            connection
                .execute(
                    r#"
                    INSERT INTO taxon_names (taxon_id, name_type, name)
                    VALUES (?, 1, ?)
                    "#,
                    params![child_taxon_id, name],
                )
                .unwrap();
            child_taxon_ids.push(child_taxon_id);
        }
        connection
            .execute(
                r#"
                INSERT INTO photo_directories (
                    parent_directory_id, name, relative_path
                ) VALUES (NULL, '', '')
                "#,
                [],
            )
            .unwrap();
        let directory_id = connection.last_insert_rowid();
        let first_photo_id = insert_test_photo(&connection, directory_id, "first.jpg");
        let second_photo_id = insert_test_photo(&connection, directory_id, "second.jpg");
        for photo_id in [first_photo_id, second_photo_id] {
            connection
                .execute(
                    r#"
                    INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                    VALUES (?, ?, 'matched')
                    "#,
                    params![photo_id, parent_taxon_id],
                )
                .unwrap();
        }
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (?, 2, 2)
                "#,
                [parent_taxon_id],
            )
            .unwrap();
        drop(connection);

        let first =
            browse_photo_taxon(&database, Some(parent_taxon_id), true, false, None, 2).unwrap();
        assert_eq!(first.items.len(), 2);
        assert!(
            first
                .items
                .iter()
                .all(|item| matches!(item, PhotoTaxonItem::Taxon { .. }))
        );
        assert!(first.next_cursor.is_some());
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO photo_mapping_queue (photo_id, reason) VALUES (?, 'refresh')",
                [first_photo_id],
            )
            .unwrap();
        let current_node = get_photo_taxon_node(&database, Some(parent_taxon_id), false).unwrap();
        assert_eq!(current_node.taxon.as_ref().unwrap().direct_photo_count, 1);
        assert_eq!(current_node.subtree_photo_count, 1);
        assert_eq!(
            get_photo_taxon_node(&database, None, false)
                .unwrap()
                .subtree_photo_count,
            1
        );
        let error = browse_photo_taxon(
            &database,
            Some(child_taxon_ids[0]),
            true,
            false,
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid photo cursor"));

        let second = browse_photo_taxon(
            &database,
            Some(parent_taxon_id),
            true,
            false,
            first.next_cursor.as_deref(),
            2,
        )
        .unwrap();
        assert_eq!(
            second.items,
            vec![PhotoTaxonItem::Photo {
                photo: photos::get_photo(&database, second_photo_id)
                    .unwrap()
                    .unwrap()
            }]
        );
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn mapping_status_pages_are_logical_and_cursor_scoped() {
        let data = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_directories (
                    parent_directory_id, name, relative_path
                ) VALUES (NULL, '', '')
                "#,
                [],
            )
            .unwrap();
        let directory_id = connection.last_insert_rowid();
        let processing_photo_id = insert_test_photo(&connection, directory_id, "processing.jpg");
        let first_unmatched_id = insert_test_photo(&connection, directory_id, "unmatched-1.jpg");
        let second_unmatched_id =
            insert_test_photo(&connection, directory_id, "Canidae-unmatched-2.jpg");
        let matched_photo_id = insert_test_photo(&connection, directory_id, "plain-match.jpg");
        let ambiguous_photo_id =
            insert_test_photo(&connection, directory_id, "plain-ambiguous.jpg");
        for photo_id in [processing_photo_id, first_unmatched_id, second_unmatched_id] {
            connection
                .execute(
                    r#"
                    INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                    VALUES (?, NULL, 'unmatched')
                    "#,
                    [photo_id],
                )
                .unwrap();
        }
        connection
            .execute_batch(&format!(
                r#"
                INSERT INTO taxa (taxon_id, rank) VALUES (1, 3), (2, 3);
                INSERT INTO taxon_names (name_id, taxon_id, name_type, name)
                VALUES
                    (1, 1, 1, 'Canidae'),
                    (2, 2, 1, 'Canidae');
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES
                    ({matched_photo_id}, 1, 'matched'),
                    ({ambiguous_photo_id}, NULL, 'ambiguous');
                INSERT INTO photo_taxon_candidates (photo_id, taxon_id)
                VALUES
                    ({ambiguous_photo_id}, 1),
                    ({ambiguous_photo_id}, 2);
                INSERT INTO photo_taxon_candidate_names (
                    photo_id, taxon_id, name_id, name_type, name
                ) VALUES
                    ({ambiguous_photo_id}, 1, 1, 1, 'Canidae'),
                    ({ambiguous_photo_id}, 2, 2, 1, 'Canidae');
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (1, 1, 1);
                "#
            ))
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_mapping_queue (photo_id, reason)
                VALUES (?, 'refresh')
                "#,
                [processing_photo_id],
            )
            .unwrap();
        drop(connection);

        let first =
            list_photos_by_mapping_status(&database, PhotoMappingListStatus::Unmatched, None, 1)
                .unwrap();
        assert_eq!(first.items[0].photo.photo_id, first_unmatched_id);
        assert_eq!(first.items[0].mapping.status, PhotoTaxonStatus::Unmatched);
        assert!(first.next_cursor.is_some());
        let error = list_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Processing,
            first.next_cursor.as_deref(),
            1,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid photo cursor"));
        let second = list_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Unmatched,
            first.next_cursor.as_deref(),
            1,
        )
        .unwrap();
        assert_eq!(second.items[0].photo.photo_id, second_unmatched_id);
        assert_eq!(second.next_cursor, None);

        let processing =
            list_photos_by_mapping_status(&database, PhotoMappingListStatus::Processing, None, 10)
                .unwrap();
        assert_eq!(processing.items.len(), 1);
        assert_eq!(processing.items[0].photo.photo_id, processing_photo_id);
        assert_eq!(
            processing.items[0].mapping.status,
            PhotoTaxonStatus::Processing
        );

        let matched_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Matched,
            "Canidae",
            None,
            10,
        )
        .unwrap();
        assert_eq!(matched_search.items.len(), 1);
        assert_eq!(matched_search.items[0].photo.photo_id, matched_photo_id);
        let unmatched_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Unmatched,
            "Canidae",
            None,
            10,
        )
        .unwrap();
        assert_eq!(unmatched_search.items.len(), 1);
        assert_eq!(
            unmatched_search.items[0].photo.photo_id,
            second_unmatched_id
        );
        let first_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Unmatched,
            "unmatched",
            None,
            1,
        )
        .unwrap();
        assert_eq!(first_search.items[0].photo.photo_id, first_unmatched_id);
        assert!(first_search.next_cursor.is_some());
        let second_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Unmatched,
            "unmatched",
            first_search.next_cursor.as_deref(),
            1,
        )
        .unwrap();
        assert_eq!(second_search.items[0].photo.photo_id, second_unmatched_id);
        assert!(second_search.next_cursor.is_none());
        let processing_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Processing,
            "processing",
            None,
            10,
        )
        .unwrap();
        assert_eq!(processing_search.items.len(), 1);
        assert_eq!(
            processing_search.items[0].photo.photo_id,
            processing_photo_id
        );
        assert!(
            search_photos_by_mapping_status(
                &database,
                PhotoMappingListStatus::Ambiguous,
                "Canidae",
                None,
                10,
            )
            .unwrap()
            .items
            .is_empty()
        );
        let ambiguous_search = search_photos_by_mapping_status(
            &database,
            PhotoMappingListStatus::Ambiguous,
            "ambiguous",
            None,
            10,
        )
        .unwrap();
        assert_eq!(ambiguous_search.items.len(), 1);
        assert_eq!(ambiguous_search.items[0].photo.photo_id, ambiguous_photo_id);
        assert!(
            search_photos_by_mapping_status(
                &database,
                PhotoMappingListStatus::Matched,
                "Canidae",
                first.next_cursor.as_deref(),
                10,
            )
            .is_err()
        );

        let metadata = get_metadata(&database).unwrap();
        assert_eq!(metadata.mapped_photo_count, 1);
        assert_eq!(metadata.unmatched_photo_count, 2);
        assert_eq!(metadata.ambiguous_photo_count, 1);
        assert_eq!(metadata.processing_photo_count, 1);
    }

    #[test]
    fn batches_usage_deltas_for_shared_ancestors() {
        let data = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (1)", [])
            .unwrap();
        let root_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO taxa (parent_taxon_id, rank) VALUES (?, 2)",
                [root_id],
            )
            .unwrap();
        let first_id = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO taxa (parent_taxon_id, rank) VALUES (?, 2)",
                [root_id],
            )
            .unwrap();
        let second_id = connection.last_insert_rowid();
        let transaction = connection.transaction().unwrap();
        let deltas = BTreeMap::from([(first_id, 1), (second_id, 1)]);

        apply_usage_deltas(&transaction, &deltas).unwrap();

        assert_eq!(
            transaction
                .query_row(
                    "SELECT subtree_photo_count FROM photo_taxon_usage WHERE taxon_id = ?",
                    [root_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
        assert_eq!(
            transaction
                .query_row(
                    "SELECT SUM(direct_photo_count) FROM photo_taxon_usage WHERE taxon_id IN (?, ?)",
                    params![first_id, second_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
    }

    #[test]
    fn large_id_sets_use_a_temporary_table() {
        let data = tempfile::tempdir().unwrap();
        let database = Database::open(data.path().join("vividarium.db")).unwrap();
        let mut connection = database.connect().unwrap();
        let transaction = connection.transaction().unwrap();
        let ids = (1..=501).collect::<Vec<_>>();
        let selection =
            id_selection(&transaction, &ids, "photo_id", "temp_mapping_photo_ids").unwrap();
        assert!(selection.values.is_empty());
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM temp_mapping_photo_ids", [], |row| row
                    .get::<_, i64>(0),)
                .unwrap(),
            501
        );
        assert!(selection.predicate.contains("temp_mapping_photo_ids"));
    }
}
