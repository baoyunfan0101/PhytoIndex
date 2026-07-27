use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{
    TaxonRank, TaxonomyNameType,
    page::{
        TaxonomyCursor, TaxonomyPage, decode_cursor, encode_cursor, invalid_cursor, page_limit,
    },
};
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonDisplayNames {
    pub sci_name: Option<String>,
    pub zh_name: Option<String>,
    pub en_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonBreadcrumbItem {
    pub taxon_id: i64,
    pub rank: TaxonRank,
    pub names: TaxonDisplayNames,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonSummary {
    pub taxon_id: i64,
    pub rank: TaxonRank,
    pub breadcrumb: Vec<TaxonBreadcrumbItem>,
    pub names: TaxonDisplayNames,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonChild {
    pub taxon_id: i64,
    pub rank: TaxonRank,
    pub names: TaxonDisplayNames,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonNameDetail {
    pub name_id: i64,
    pub name: String,
    pub authority_year: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonNamesDetail {
    pub sci_name: Option<TaxonNameDetail>,
    pub synonyms: Vec<TaxonNameDetail>,
    pub zh_name: Option<TaxonNameDetail>,
    pub zh_aliases: Vec<TaxonNameDetail>,
    pub en_name: Option<TaxonNameDetail>,
    pub en_aliases: Vec<TaxonNameDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonDetail {
    pub taxon_id: i64,
    pub rank: TaxonRank,
    pub parent_taxon_id: Option<i64>,
    pub geological_range: Option<String>,
    pub names: TaxonNamesDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonDetailNode {
    pub summary: TaxonSummary,
    pub detail: TaxonDetail,
    pub children: TaxonomyPage<TaxonChild>,
}

pub fn get_taxon_summary(database: &Database, taxon_id: i64) -> CoreResult<Option<TaxonSummary>> {
    load_taxon_summary(&database.connect()?, taxon_id)
}

pub fn get_taxon_detail(database: &Database, taxon_id: i64) -> CoreResult<Option<TaxonDetail>> {
    load_taxon_detail(&database.connect()?, taxon_id)
}

pub fn get_taxon_detail_node(
    database: &Database,
    taxon_id: i64,
    children_cursor: Option<&str>,
    children_limit: usize,
) -> CoreResult<Option<TaxonDetailNode>> {
    let connection = database.connect()?;
    let Some(summary) = load_taxon_summary(&connection, taxon_id)? else {
        return Ok(None);
    };
    let detail = load_taxon_detail(&connection, taxon_id)?.ok_or_else(|| {
        CoreError::InvalidArgument(format!("taxon {taxon_id} disappeared while loading"))
    })?;
    let children = load_taxon_children(&connection, taxon_id, children_cursor, children_limit)?;
    Ok(Some(TaxonDetailNode {
        summary,
        detail,
        children,
    }))
}

pub fn list_taxon_children(
    database: &Database,
    taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonChild>> {
    load_taxon_children(&database.connect()?, taxon_id, cursor, limit)
}

pub(super) fn load_taxon_summary(
    connection: &Connection,
    taxon_id: i64,
) -> CoreResult<Option<TaxonSummary>> {
    let Some((rank, parent_taxon_id)) = load_taxon_base(connection, taxon_id)? else {
        return Ok(None);
    };
    let names = load_display_names(connection, taxon_id)?;
    let mut breadcrumb = Vec::new();
    let mut current = parent_taxon_id;
    let mut seen = HashSet::from([taxon_id]);
    while let Some(parent_id) = current {
        if !seen.insert(parent_id) {
            return Err(CoreError::InvalidArgument(
                "taxonomy hierarchy contains a cycle".into(),
            ));
        }
        let Some((parent_rank, next_parent)) = load_taxon_base(connection, parent_id)? else {
            return Err(CoreError::InvalidArgument(format!(
                "taxon {taxon_id} references missing parent {parent_id}"
            )));
        };
        breadcrumb.push(TaxonBreadcrumbItem {
            taxon_id: parent_id,
            rank: parent_rank,
            names: load_display_names(connection, parent_id)?,
        });
        current = next_parent;
    }
    breadcrumb.reverse();
    Ok(Some(TaxonSummary {
        taxon_id,
        rank,
        breadcrumb,
        names,
    }))
}

pub(crate) fn load_taxon_summaries(
    connection: &Connection,
    taxon_ids: &[i64],
) -> CoreResult<Vec<TaxonSummary>> {
    taxon_ids
        .iter()
        .map(|taxon_id| {
            load_taxon_summary(connection, *taxon_id)?.ok_or_else(|| {
                CoreError::InvalidArgument(format!("taxon {taxon_id} no longer exists"))
            })
        })
        .collect()
}

pub(super) fn load_taxon_details(
    connection: &Connection,
    taxon_ids: &[i64],
) -> CoreResult<Vec<TaxonDetail>> {
    taxon_ids
        .iter()
        .map(|taxon_id| {
            load_taxon_detail(connection, *taxon_id)?.ok_or_else(|| {
                CoreError::InvalidArgument(format!("taxon {taxon_id} no longer exists"))
            })
        })
        .collect()
}

fn load_taxon_detail(connection: &Connection, taxon_id: i64) -> CoreResult<Option<TaxonDetail>> {
    let base = connection
        .query_row(
            "SELECT rank, parent_taxon_id, geological_range FROM taxa WHERE taxon_id = ?",
            [taxon_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((rank, parent_taxon_id, geological_range)) = base else {
        return Ok(None);
    };
    Ok(Some(TaxonDetail {
        taxon_id,
        rank: TaxonRank::from_code(rank)?,
        parent_taxon_id,
        geological_range,
        names: load_name_details(connection, taxon_id)?,
    }))
}

fn load_taxon_children(
    connection: &Connection,
    parent_taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonChild>> {
    let cursor = match decode_cursor(cursor)? {
        Some(TaxonomyCursor::TaxonChildren {
            parent_taxon_id: cursor_parent,
            rank,
            taxon_id,
        }) if cursor_parent == parent_taxon_id => Some((rank, taxon_id)),
        Some(_) => return Err(invalid_cursor()),
        None => None,
    };
    let limit = page_limit(limit);
    let mut rows = match cursor {
        Some((rank, taxon_id)) => {
            let mut statement = connection.prepare(
                r#"
                SELECT taxon_id, rank
                FROM taxa
                WHERE parent_taxon_id = ?
                  AND (rank > ? OR (rank = ? AND taxon_id > ?))
                ORDER BY rank, taxon_id
                LIMIT ?
                "#,
            )?;
            statement
                .query_map(
                    params![
                        parent_taxon_id,
                        rank.code(),
                        rank.code(),
                        taxon_id,
                        (limit + 1) as i64
                    ],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )?
                .collect::<Result<Vec<_>, _>>()?
        }
        None => {
            let mut statement = connection.prepare(
                r#"
                SELECT taxon_id, rank
                FROM taxa
                WHERE parent_taxon_id = ?
                ORDER BY rank, taxon_id
                LIMIT ?
                "#,
            )?;
            statement
                .query_map(params![parent_taxon_id, (limit + 1) as i64], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let mut items = Vec::with_capacity(rows.len());
    for (taxon_id, rank) in rows {
        items.push(TaxonChild {
            taxon_id,
            rank: TaxonRank::from_code(rank)?,
            names: load_display_names(connection, taxon_id)?,
        });
    }
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| {
                encode_cursor(&TaxonomyCursor::TaxonChildren {
                    parent_taxon_id,
                    rank: item.rank,
                    taxon_id: item.taxon_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(TaxonomyPage { items, next_cursor })
}

fn load_taxon_base(
    connection: &Connection,
    taxon_id: i64,
) -> CoreResult<Option<(TaxonRank, Option<i64>)>> {
    let value = connection
        .query_row(
            "SELECT rank, parent_taxon_id FROM taxa WHERE taxon_id = ?",
            [taxon_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()?;
    value
        .map(|(rank, parent)| Ok((TaxonRank::from_code(rank)?, parent)))
        .transpose()
}

fn load_display_names(connection: &Connection, taxon_id: i64) -> CoreResult<TaxonDisplayNames> {
    let mut names = TaxonDisplayNames::default();
    let mut statement = connection.prepare(
        r#"
        SELECT name_type, name
        FROM taxon_names
        WHERE taxon_id = ? AND name_type IN (1, 3, 5)
        "#,
    )?;
    let rows = statement.query_map([taxon_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (name_type, name) = row?;
        match TaxonomyNameType::from_code(name_type)? {
            TaxonomyNameType::SciName => names.sci_name = Some(name),
            TaxonomyNameType::ZhName => names.zh_name = Some(name),
            TaxonomyNameType::EnName => names.en_name = Some(name),
            _ => {}
        }
    }
    Ok(names)
}

fn load_name_details(connection: &Connection, taxon_id: i64) -> CoreResult<TaxonNamesDetail> {
    let mut result = TaxonNamesDetail::default();
    let mut statement = connection.prepare(
        r#"
        SELECT name_id, name_type, name, authority_year, source
        FROM taxon_names
        WHERE taxon_id = ?
        ORDER BY name_type, name
        "#,
    )?;
    let rows = statement.query_map([taxon_id], |row| {
        Ok((
            row.get::<_, i64>(1)?,
            TaxonNameDetail {
                name_id: row.get(0)?,
                name: row.get(2)?,
                authority_year: row.get(3)?,
                source: row.get(4)?,
            },
        ))
    })?;
    for row in rows {
        let (name_type, detail) = row?;
        match TaxonomyNameType::from_code(name_type)? {
            TaxonomyNameType::SciName => result.sci_name = Some(detail),
            TaxonomyNameType::Synonym => result.synonyms.push(detail),
            TaxonomyNameType::ZhName => result.zh_name = Some(detail),
            TaxonomyNameType::ZhAlias => result.zh_aliases.push(detail),
            TaxonomyNameType::EnName => result.en_name = Some(detail),
            TaxonomyNameType::EnAlias => result.en_aliases.push(detail),
        }
    }
    Ok(result)
}
