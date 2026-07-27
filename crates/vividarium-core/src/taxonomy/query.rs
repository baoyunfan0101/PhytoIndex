use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, functions::FunctionFlags, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use super::{
    TaxonomyNameType, page::page_limit, view::load_taxon_details, view::load_taxon_summaries,
};
use crate::naming::normalize_taxonomy_name;
use crate::{CoreError, CoreResult, Database};

const FUZZY_MATCH_LEVEL: i64 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonNameMatch {
    pub name_id: i64,
    pub name_type: TaxonomyNameType,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonSearchResult {
    pub summary: super::TaxonSummary,
    pub detail: super::TaxonDetail,
    pub matches: Vec<TaxonNameMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonSuggestion {
    pub taxon_id: i64,
    pub rank: super::TaxonRank,
    pub names: super::TaxonDisplayNames,
    pub matches: Vec<TaxonNameMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TaxonSearchCursorKey {
    pub(crate) match_level: i64,
    pub(crate) edit_distance: i64,
    pub(crate) sort_name: String,
    pub(crate) name_type_priority: i64,
    pub(crate) taxon_id: i64,
}

#[derive(Debug)]
pub(crate) struct RankedTaxonSearchResult {
    pub(crate) result: TaxonSearchResult,
    pub(crate) key: TaxonSearchCursorKey,
}

pub(crate) struct TaxonSearchRelation {
    pub(crate) cte_sql: String,
    pub(crate) params: Vec<SqlValue>,
}

pub fn search_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSearchResult>> {
    let connection = database.connect_taxonomy_context()?;
    search_taxa_with_connection(&connection, query, limit)
}

pub(crate) fn search_taxa_with_connection(
    connection: &Connection,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSearchResult>> {
    Ok(
        search_taxa_with_filter(connection, query, None, page_limit(limit), false)?
            .into_iter()
            .map(|result| result.result)
            .collect(),
    )
}

pub(crate) fn search_taxa_page_with_photos_connection(
    connection: &Connection,
    query: &str,
    after: Option<&TaxonSearchCursorKey>,
    limit: usize,
) -> CoreResult<Vec<RankedTaxonSearchResult>> {
    search_taxa_with_filter(connection, query, after, limit, true)
}

pub fn suggest_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSuggestion>> {
    suggest_taxa_with_filter(
        &database.connect_taxonomy_context()?,
        query,
        page_limit(limit),
        false,
    )
}

pub(crate) fn suggest_taxa_with_photos_connection(
    connection: &Connection,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSuggestion>> {
    suggest_taxa_with_filter(connection, query, page_limit(limit), true)
}

pub(crate) fn taxon_search_relation(
    connection: &Connection,
    query: &str,
) -> CoreResult<TaxonSearchRelation> {
    let Some(query) = normalize_search_query(query) else {
        return Ok(empty_taxon_search_relation());
    };
    register_search_functions(connection)?;
    Ok(build_taxon_search_relation(&SearchQuery::new(&query)))
}

fn search_taxa_with_filter(
    connection: &Connection,
    query: &str,
    after: Option<&TaxonSearchCursorKey>,
    limit: usize,
    require_photos: bool,
) -> CoreResult<Vec<RankedTaxonSearchResult>> {
    let Some(query) = normalize_search_query(query) else {
        return Ok(Vec::new());
    };
    let search = SearchQuery::new(&query);
    let search_matches = search_ranked_taxa(connection, &search, after, limit, require_photos)?;
    let ids = search_matches
        .iter()
        .map(|matched| matched.key.taxon_id)
        .collect::<Vec<_>>();
    let summaries = load_taxon_summaries(connection, &ids)?;
    let details = load_taxon_details(connection, &ids)?;
    let fuzzy_taxon_ids = search_matches
        .iter()
        .filter(|matched| matched.key.match_level == FUZZY_MATCH_LEVEL)
        .map(|matched| matched.key.taxon_id)
        .collect::<HashSet<_>>();
    let matches_by_id = load_name_matches_for_taxa(connection, &ids, &search, &fuzzy_taxon_ids)?;
    if summaries.len() != ids.len() || details.len() != ids.len() {
        return Err(CoreError::InvalidArgument(
            "matched taxon no longer exists".into(),
        ));
    }
    search_matches
        .into_iter()
        .zip(summaries)
        .zip(details)
        .map(|((matched, summary), detail)| {
            Ok(RankedTaxonSearchResult {
                result: TaxonSearchResult {
                    summary,
                    detail,
                    matches: matches_by_id
                        .get(&matched.key.taxon_id)
                        .cloned()
                        .unwrap_or_default(),
                },
                key: matched.key,
            })
        })
        .collect()
}

fn suggest_taxa_with_filter(
    connection: &Connection,
    query: &str,
    limit: usize,
    require_photos: bool,
) -> CoreResult<Vec<TaxonSuggestion>> {
    let Some(query) = normalize_search_query(query) else {
        return Ok(Vec::new());
    };
    let search = SearchQuery::new(&query);
    let search_matches = search_ranked_taxa(connection, &search, None, limit, require_photos)?;
    let ids = search_matches
        .iter()
        .map(|matched| matched.key.taxon_id)
        .collect::<Vec<_>>();
    let mut suggestions = load_suggestions(connection, &ids)?;
    let fuzzy_taxon_ids = search_matches
        .iter()
        .filter(|matched| matched.key.match_level == FUZZY_MATCH_LEVEL)
        .map(|matched| matched.key.taxon_id)
        .collect::<HashSet<_>>();
    let matches_by_id = load_name_matches_for_taxa(connection, &ids, &search, &fuzzy_taxon_ids)?;
    for suggestion in &mut suggestions {
        suggestion.matches = matches_by_id
            .get(&suggestion.taxon_id)
            .cloned()
            .unwrap_or_default();
    }
    Ok(suggestions)
}

fn load_suggestions(
    connection: &Connection,
    taxon_ids: &[i64],
) -> CoreResult<Vec<TaxonSuggestion>> {
    if taxon_ids.is_empty() {
        return Ok(Vec::new());
    }
    let values_clause = taxon_ids
        .iter()
        .map(|_| "(?, ?)")
        .collect::<Vec<_>>()
        .join(", ");
    let mut values = Vec::with_capacity(taxon_ids.len() * 2);
    for (sort_order, taxon_id) in taxon_ids.iter().enumerate() {
        values.push(SqlValue::Integer(*taxon_id));
        values.push(SqlValue::Integer(sort_order as i64));
    }
    let sql = format!(
        r#"
        WITH input(taxon_id, sort_order) AS (VALUES {values_clause})
        SELECT taxa.taxon_id, taxa.rank,
               MAX(CASE WHEN taxon_names.name_type = 1 THEN taxon_names.name END),
               MAX(CASE WHEN taxon_names.name_type = 3 THEN taxon_names.name END),
               MAX(CASE WHEN taxon_names.name_type = 5 THEN taxon_names.name END)
        FROM input
        JOIN taxa USING (taxon_id)
        LEFT JOIN taxon_names
          ON taxon_names.taxon_id = taxa.taxon_id
         AND taxon_names.name_type IN (1, 3, 5)
        GROUP BY input.sort_order, taxa.taxon_id, taxa.rank
        ORDER BY input.sort_order
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), |row| {
        let rank = super::TaxonRank::from_code(row.get::<_, i64>(1)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Integer,
                error.to_string().into(),
            )
        })?;
        Ok(TaxonSuggestion {
            taxon_id: row.get(0)?,
            rank,
            names: super::TaxonDisplayNames {
                sci_name: row.get(2)?,
                zh_name: row.get(3)?,
                en_name: row.get(4)?,
            },
            matches: Vec::new(),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone)]
struct SearchQuery {
    normalized: String,
    prefix_upper: String,
    word_prefix_match: Option<String>,
    contains_match: Option<String>,
    fuzzy_match: Option<String>,
    fuzzy_max_distance: usize,
    word_prefix_like_pattern: Option<String>,
    contains_like_pattern: Option<String>,
}

impl SearchQuery {
    fn new(query: &str) -> Self {
        let normalized = query.to_ascii_lowercase();
        let char_count = query.chars().count();
        Self {
            prefix_upper: format!("{normalized}\u{10ffff}"),
            word_prefix_match: (char_count >= 2).then(|| quoted_fts_match(&format!(" {query}"))),
            contains_match: (char_count >= 3).then(|| quoted_fts_match(query)),
            fuzzy_match: trigram_match_query(&normalized),
            fuzzy_max_distance: fuzzy_max_distance(char_count),
            word_prefix_like_pattern: (char_count >= 2)
                .then(|| format!("% {}%", escape_like(query))),
            contains_like_pattern: (char_count >= 3).then(|| format!("%{}%", escape_like(query))),
            normalized,
        }
    }
}

#[derive(Debug)]
struct RankedTaxonMatch {
    key: TaxonSearchCursorKey,
}

fn search_ranked_taxa(
    connection: &Connection,
    search: &SearchQuery,
    after: Option<&TaxonSearchCursorKey>,
    limit: usize,
    require_photos: bool,
) -> CoreResult<Vec<RankedTaxonMatch>> {
    register_search_functions(connection)?;
    let mut relation = build_taxon_search_relation(search);
    let photo_filter = photo_filter("ranked_taxa", require_photos);
    let cursor_filter = if let Some(after) = after {
        relation.params.extend([
            SqlValue::Integer(after.match_level),
            SqlValue::Integer(after.edit_distance),
            SqlValue::Text(after.sort_name.clone()),
            SqlValue::Integer(after.name_type_priority),
            SqlValue::Integer(after.taxon_id),
        ]);
        r#"
        AND (
            ranked_taxa.match_level,
            ranked_taxa.edit_distance,
            ranked_taxa.sort_name,
            ranked_taxa.name_type_priority,
            ranked_taxa.taxon_id
        ) > (?, ?, ?, ?, ?)
        "#
    } else {
        ""
    };
    relation.params.push(SqlValue::Integer(limit as i64));
    let sql = format!(
        r#"
        WITH {ctes}
        SELECT taxon_id, match_level, edit_distance, sort_name,
               name_type_priority
        FROM ranked_taxa
        WHERE 1 = 1
          {photo_filter}
          {cursor_filter}
        ORDER BY match_level, edit_distance, sort_name,
                 name_type_priority, taxon_id
        LIMIT ?
        "#,
        ctes = relation.cte_sql,
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(relation.params), |row| {
        Ok(RankedTaxonMatch {
            key: TaxonSearchCursorKey {
                taxon_id: row.get(0)?,
                match_level: row.get(1)?,
                edit_distance: row.get(2)?,
                sort_name: row.get(3)?,
                name_type_priority: row.get(4)?,
            },
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn build_taxon_search_relation(search: &SearchQuery) -> TaxonSearchRelation {
    let mut candidates = Vec::new();
    let mut params = Vec::new();
    candidates.push(
        r#"
        SELECT name_id, taxon_id, 0 AS match_level, 0 AS edit_distance,
               normalized_name AS sort_name,
               CASE name_type WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END
                   AS name_type_priority
        FROM taxon_names
        WHERE normalized_name = ?
        "#
        .to_string(),
    );
    params.push(SqlValue::Text(search.normalized.clone()));

    candidates.push(
        r#"
        SELECT name_id, taxon_id, 1 AS match_level, 0 AS edit_distance,
               normalized_name AS sort_name,
               CASE name_type WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END
                   AS name_type_priority
        FROM taxon_names
        WHERE normalized_name >= ?
          AND normalized_name < ?
          AND normalized_name != ?
        "#
        .to_string(),
    );
    params.extend([
        SqlValue::Text(search.normalized.clone()),
        SqlValue::Text(search.prefix_upper.clone()),
        SqlValue::Text(search.normalized.clone()),
    ]);

    if let Some(query) = search.word_prefix_match.as_ref() {
        candidates.push(
            r#"
            SELECT taxon_names.name_id, taxon_names.taxon_id,
                   2 AS match_level, 0 AS edit_distance,
                   taxon_names.normalized_name AS sort_name,
                   CASE taxon_names.name_type
                       WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END
                       AS name_type_priority
            FROM taxon_names_fts
            JOIN taxon_names ON taxon_names.name_id = taxon_names_fts.rowid
            WHERE taxon_names_fts MATCH ?
              AND NOT (
                  taxon_names.normalized_name >= ?
                  AND taxon_names.normalized_name < ?
              )
            "#
            .to_string(),
        );
        params.extend([
            SqlValue::Text(query.clone()),
            SqlValue::Text(search.normalized.clone()),
            SqlValue::Text(search.prefix_upper.clone()),
        ]);
    }

    if let Some(query) = search.contains_match.as_ref() {
        let word_exclusion = if search.word_prefix_like_pattern.is_some() {
            "AND taxon_names.normalized_name NOT LIKE ? ESCAPE '\\'"
        } else {
            ""
        };
        candidates.push(format!(
            r#"
            SELECT taxon_names.name_id, taxon_names.taxon_id,
                   3 AS match_level, 0 AS edit_distance,
                   taxon_names.normalized_name AS sort_name,
                   CASE taxon_names.name_type
                       WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END
                       AS name_type_priority
            FROM taxon_names_fts
            JOIN taxon_names ON taxon_names.name_id = taxon_names_fts.rowid
            WHERE taxon_names_fts MATCH ?
              AND NOT (
                  taxon_names.normalized_name >= ?
                  AND taxon_names.normalized_name < ?
              )
              {word_exclusion}
            "#
        ));
        params.extend([
            SqlValue::Text(query.clone()),
            SqlValue::Text(search.normalized.clone()),
            SqlValue::Text(search.prefix_upper.clone()),
        ]);
        if let Some(pattern) = search.word_prefix_like_pattern.as_ref() {
            params.push(SqlValue::Text(pattern.clone()));
        }
    }

    if let (Some(query), Some(contains_pattern)) = (
        search.fuzzy_match.as_ref(),
        search.contains_like_pattern.as_ref(),
    ) {
        candidates.push(
            r#"
            SELECT taxon_names.name_id, taxon_names.taxon_id,
                   4 AS match_level,
                   taxonomy_edit_distance(
                       taxon_names.normalized_name, ?, ?
                   ) AS edit_distance,
                   taxon_names.normalized_name AS sort_name,
                   CASE taxon_names.name_type
                       WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END
                       AS name_type_priority
            FROM taxon_names_fts
            JOIN taxon_names ON taxon_names.name_id = taxon_names_fts.rowid
            WHERE taxon_names_fts MATCH ?
              AND taxonomy_edit_distance(
                  taxon_names.normalized_name, ?, ?
              ) IS NOT NULL
              AND taxon_names.normalized_name NOT LIKE ? ESCAPE '\'
            "#
            .to_string(),
        );
        params.extend([
            SqlValue::Text(search.normalized.clone()),
            SqlValue::Integer(search.fuzzy_max_distance as i64),
            SqlValue::Text(query.clone()),
            SqlValue::Text(search.normalized.clone()),
            SqlValue::Integer(search.fuzzy_max_distance as i64),
            SqlValue::Text(contains_pattern.clone()),
        ]);
    }

    TaxonSearchRelation {
        cte_sql: format!(
            r#"
            search_name_candidates AS (
                {}
            ),
            ranked_search_names AS (
                SELECT name_id, taxon_id, match_level, edit_distance,
                       sort_name, name_type_priority,
                       ROW_NUMBER() OVER (
                           PARTITION BY taxon_id
                           ORDER BY match_level, edit_distance, sort_name,
                                    name_type_priority, name_id
                       ) AS name_rank
                FROM search_name_candidates
            ),
            ranked_taxa AS (
                SELECT name_id, taxon_id, match_level, edit_distance,
                       sort_name, name_type_priority
                FROM ranked_search_names
                WHERE name_rank = 1
            )
            "#,
            candidates.join("\nUNION ALL\n")
        ),
        params,
    }
}

fn empty_taxon_search_relation() -> TaxonSearchRelation {
    TaxonSearchRelation {
        cte_sql: r#"
            ranked_taxa(
                name_id, taxon_id, match_level, edit_distance,
                sort_name, name_type_priority
            ) AS (
                SELECT NULL, NULL, NULL, NULL, NULL, NULL
                WHERE 0
            )
        "#
        .to_string(),
        params: Vec::new(),
    }
}

fn register_search_functions(connection: &Connection) -> CoreResult<()> {
    connection.create_scalar_function(
        "taxonomy_edit_distance",
        3,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let left = context.get::<String>(0)?;
            let right = context.get::<String>(1)?;
            let limit = context.get::<i64>(2)?;
            Ok(
                edit_distance_with_limit(&left, &right, limit.max(0) as usize)
                    .map(|distance| distance as i64),
            )
        },
    )?;
    Ok(())
}

fn photo_filter(table_name: &str, require_photos: bool) -> String {
    if !require_photos {
        return String::new();
    }
    format!(
        r#"
        AND EXISTS (
            SELECT 1
            FROM current_photo_taxon_usage
            WHERE current_photo_taxon_usage.taxon_id = {table_name}.taxon_id
              AND current_photo_taxon_usage.subtree_photo_count > 0
        )
        "#
    )
}

fn load_name_matches_for_taxa(
    connection: &Connection,
    taxon_ids: &[i64],
    search: &SearchQuery,
    fuzzy_taxon_ids: &HashSet<i64>,
) -> CoreResult<HashMap<i64, Vec<TaxonNameMatch>>> {
    if taxon_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let values_clause = taxon_ids
        .iter()
        .map(|_| "(?, ?)")
        .collect::<Vec<_>>()
        .join(", ");
    let mut matches_by_id: HashMap<i64, Vec<TaxonNameMatch>> = HashMap::new();
    let mut query_params = Vec::with_capacity(taxon_ids.len() * 2 + 5);
    for (index, taxon_id) in taxon_ids.iter().enumerate() {
        query_params.push(SqlValue::Integer(*taxon_id));
        query_params.push(SqlValue::Integer(index as i64));
    }
    query_params.push(SqlValue::Text(search.normalized.clone()));
    query_params.push(SqlValue::Text(search.prefix_upper.clone()));
    let mut conditions =
        vec!["(taxon_names.normalized_name >= ? AND taxon_names.normalized_name < ?)".to_string()];
    if let Some(pattern) = search.word_prefix_like_pattern.as_ref() {
        conditions.push("taxon_names.name LIKE ? ESCAPE '\\'".to_string());
        query_params.push(SqlValue::Text(pattern.clone()));
    }
    if let Some(pattern) = search.contains_like_pattern.as_ref() {
        conditions.push("taxon_names.name LIKE ? ESCAPE '\\'".to_string());
        query_params.push(SqlValue::Text(pattern.clone()));
    }
    if !fuzzy_taxon_ids.is_empty() {
        let placeholders = vec!["?"; fuzzy_taxon_ids.len()].join(", ");
        conditions.push(format!(
            r#"
            (
                taxon_names.taxon_id IN ({placeholders})
                AND taxonomy_edit_distance(
                    taxon_names.normalized_name, ?, ?
                ) IS NOT NULL
            )
            "#
        ));
        let mut taxon_ids = fuzzy_taxon_ids.iter().copied().collect::<Vec<_>>();
        taxon_ids.sort_unstable();
        query_params.extend(taxon_ids.into_iter().map(SqlValue::Integer));
        query_params.push(SqlValue::Text(search.normalized.clone()));
        query_params.push(SqlValue::Integer(search.fuzzy_max_distance as i64));
    }
    let conditions = conditions.join(" OR ");
    let sql = format!(
        r#"
        WITH input(taxon_id, sort_order) AS (VALUES {values_clause})
        SELECT input.taxon_id, taxon_names.name_id, taxon_names.name_type,
               taxon_names.name
        FROM input
        JOIN taxon_names ON taxon_names.taxon_id = input.taxon_id
        WHERE {conditions}
        ORDER BY input.sort_order,
                 CASE taxon_names.name_type
                     WHEN 1 THEN 0 WHEN 2 THEN 1 ELSE 2 END,
                 taxon_names.name
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(query_params), |row| {
        let name_type = TaxonomyNameType::from_code(row.get::<_, i64>(2)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Integer,
                error.to_string().into(),
            )
        })?;
        Ok((
            row.get::<_, i64>(0)?,
            TaxonNameMatch {
                name_id: row.get(1)?,
                name_type,
                name: row.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (taxon_id, name_match) = row?;
        matches_by_id.entry(taxon_id).or_default().push(name_match);
    }
    Ok(matches_by_id)
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn normalize_search_query(value: &str) -> Option<String> {
    normalize_taxonomy_name(value)
}

fn quoted_fts_match(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn trigram_match_query(value: &str) -> Option<String> {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.len() < 3 {
        return None;
    }
    let mut seen = HashSet::new();
    let trigrams = characters
        .windows(3)
        .map(|window| window.iter().collect::<String>())
        .filter(|trigram| seen.insert(trigram.clone()))
        .map(|trigram| quoted_fts_match(&trigram))
        .collect::<Vec<_>>();
    Some(trigrams.join(" OR "))
}

fn fuzzy_max_distance(char_count: usize) -> usize {
    match char_count {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    }
}

fn edit_distance_with_limit(left: &str, right: &str, limit: usize) -> Option<usize> {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.len().abs_diff(right.len()) > limit {
        return None;
    }

    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        let mut row_minimum = current[0];
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution_cost = usize::from(left_char != right_char);
            current[right_index + 1] = (current[right_index] + 1)
                .min(previous[right_index + 1] + 1)
                .min(previous[right_index] + substitution_cost);
            row_minimum = row_minimum.min(current[right_index + 1]);
        }
        if row_minimum > limit {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    (previous[right.len()] <= limit).then_some(previous[right.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_share_search_order_and_only_load_compact_fields() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        database
            .connect()
            .unwrap()
            .execute_batch(
                r#"
                INSERT INTO taxa (taxon_id, parent_taxon_id, rank) VALUES
                    (1, NULL, 4),
                    (2, NULL, 4);
                INSERT INTO taxon_names (taxon_id, name_type, name) VALUES
                    (1, 1, 'Canis'),
                    (1, 3, 'Dogs'),
                    (2, 1, 'Lycaon'),
                    (2, 2, 'Canis');
                "#,
            )
            .unwrap();

        let search_ids = search_taxa(&database, "Canis", 10)
            .unwrap()
            .into_iter()
            .map(|result| result.summary.taxon_id)
            .collect::<Vec<_>>();
        let suggestions = suggest_taxa(&database, "Canis", 10).unwrap();
        assert_eq!(
            suggestions
                .iter()
                .map(|suggestion| suggestion.taxon_id)
                .collect::<Vec<_>>(),
            search_ids
        );
        assert_eq!(suggestions[0].names.sci_name.as_deref(), Some("Canis"));
        assert_eq!(suggestions[0].names.zh_name.as_deref(), Some("Dogs"));
        assert_eq!(suggestions[0].matches[0].name, "Canis");
    }

    #[test]
    fn ranked_search_cursor_continues_across_match_levels() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let connection = database.connect_taxonomy_context().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO taxa (taxon_id, parent_taxon_id, rank) VALUES
                    (10, NULL, 5),
                    (11, NULL, 5),
                    (12, NULL, 5),
                    (13, NULL, 5),
                    (14, NULL, 5);
                INSERT INTO taxon_names (taxon_id, name_type, name) VALUES
                    (10, 1, 'Canis'),
                    (11, 1, 'Canis lupus'),
                    (12, 1, 'Great Canis wolf'),
                    (13, 1, 'Toucanis'),
                    (14, 1, 'Canos');
                "#,
            )
            .unwrap();
        let search = SearchQuery::new("Canis");

        let first = search_ranked_taxa(&connection, &search, None, 2, false).unwrap();
        assert_eq!(
            first
                .iter()
                .map(|result| result.key.match_level)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let second =
            search_ranked_taxa(&connection, &search, Some(&first[1].key), 2, false).unwrap();
        assert_eq!(
            second
                .iter()
                .map(|result| result.key.match_level)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        let third =
            search_ranked_taxa(&connection, &search, Some(&second[1].key), 2, false).unwrap();
        assert_eq!(
            third
                .iter()
                .map(|result| (result.key.taxon_id, result.key.match_level))
                .collect::<Vec<_>>(),
            vec![(14, FUZZY_MATCH_LEVEL)]
        );
    }
}
