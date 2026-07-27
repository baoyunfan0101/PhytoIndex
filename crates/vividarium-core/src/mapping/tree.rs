use rusqlite::{OptionalExtension, params, types::Value as SqlValue};

use super::{PhotoTaxonItem, PhotoTaxonNode, PhotoTaxonUsage};
use crate::models::{Photo, PhotoPage};
use crate::photos::{
    PhotoCursor, PhotoPageSection, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor,
    photo_page_limit,
};
use crate::taxonomy::{TaxonDisplayNames, TaxonRank};
use crate::{CoreError, CoreResult, Database};

pub fn get_photo_taxon_node(
    database: &Database,
    taxon_id: Option<i64>,
    show_empty: bool,
) -> CoreResult<PhotoTaxonNode> {
    let connection = database.connect()?;
    let taxon = match taxon_id {
        Some(taxon_id) => load_usage_taxon(&connection, taxon_id, show_empty)?
            .ok_or_else(|| CoreError::NotFound(format!("photo taxon node {taxon_id}")))?,
        None => {
            let subtree_photo_count = connection.query_row(
                "SELECT COUNT(*) FROM current_photo_taxon_mapping",
                [],
                |row| row.get(0),
            )?;
            return Ok(PhotoTaxonNode {
                taxon: None,
                subtree_photo_count,
            });
        }
    };
    let subtree_photo_count = taxon.subtree_photo_count;
    Ok(PhotoTaxonNode {
        taxon: Some(taxon),
        subtree_photo_count,
    })
}

pub fn browse_photo_taxon(
    database: &Database,
    taxon_id: Option<i64>,
    show_empty: bool,
    include_descendants: bool,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoTaxonItem>> {
    let connection = database.connect()?;
    if let Some(taxon_id) = taxon_id {
        load_usage_taxon(&connection, taxon_id, show_empty)?
            .ok_or_else(|| CoreError::NotFound(format!("photo taxon node {taxon_id}")))?;
    }
    let (section, after_rank, after_item_id) = match decode_photo_cursor(cursor)? {
        None => (PhotoPageSection::Containers, 0, 0),
        Some(PhotoCursor::TaxonEntries {
            taxon_id: cursor_taxon_id,
            show_empty: cursor_show_empty,
            include_descendants: cursor_include_descendants,
            section,
            rank,
            item_id,
        }) if cursor_taxon_id == taxon_id
            && cursor_show_empty == show_empty
            && cursor_include_descendants == include_descendants =>
        {
            (section, rank, item_id)
        }
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let mut remaining = limit;
    let mut taxa = Vec::new();
    let mut has_more = false;
    if section == PhotoPageSection::Containers {
        taxa = load_usage_children_page(
            &connection,
            taxon_id,
            show_empty,
            after_rank,
            after_item_id,
            remaining + 1,
        )?;
        if taxa.len() > remaining {
            taxa.pop();
            let next_cursor = taxa
                .last()
                .map(|taxon| {
                    encode_photo_cursor(&PhotoCursor::TaxonEntries {
                        taxon_id,
                        show_empty,
                        include_descendants,
                        section: PhotoPageSection::Containers,
                        rank: taxon.rank.code(),
                        item_id: taxon.taxon_id,
                    })
                })
                .transpose()?;
            return Ok(PhotoPage {
                items: taxa
                    .into_iter()
                    .map(|taxon| PhotoTaxonItem::Taxon { taxon })
                    .collect(),
                next_cursor,
            });
        }
        remaining -= taxa.len();
    }
    let after_photo_id = if section == PhotoPageSection::Photos {
        after_item_id
    } else {
        0
    };
    let mut photos = load_photos_for_taxon(
        &connection,
        taxon_id,
        include_descendants,
        after_photo_id,
        remaining + 1,
    )?;
    if photos.len() > remaining {
        photos.pop();
        has_more = true;
    }
    let next_cursor = if has_more {
        if let Some(photo) = photos.last() {
            Some(encode_photo_cursor(&PhotoCursor::TaxonEntries {
                taxon_id,
                show_empty,
                include_descendants,
                section: PhotoPageSection::Photos,
                rank: 0,
                item_id: photo.photo_id,
            })?)
        } else {
            taxa.last()
                .map(|taxon| {
                    encode_photo_cursor(&PhotoCursor::TaxonEntries {
                        taxon_id,
                        show_empty,
                        include_descendants,
                        section: PhotoPageSection::Containers,
                        rank: taxon.rank.code(),
                        item_id: taxon.taxon_id,
                    })
                })
                .transpose()?
        }
    } else {
        None
    };
    let mut items = taxa
        .into_iter()
        .map(|taxon| PhotoTaxonItem::Taxon { taxon })
        .collect::<Vec<_>>();
    items.extend(
        photos
            .into_iter()
            .map(|photo| PhotoTaxonItem::Photo { photo }),
    );
    Ok(PhotoPage { items, next_cursor })
}

fn load_photos_for_taxon(
    connection: &rusqlite::Connection,
    taxon_id: Option<i64>,
    include_descendants: bool,
    after_photo_id: i64,
    limit: usize,
) -> CoreResult<Vec<Photo>> {
    let suffix = match (taxon_id, include_descendants) {
        (Some(_), true) => {
            r#"
            JOIN current_photo_taxon_mapping
              ON current_photo_taxon_mapping.photo_id = photos.photo_id
            JOIN (
                WITH RECURSIVE descendants(taxon_id) AS (
                    SELECT taxon_id FROM taxa WHERE taxon_id = ?1
                    UNION ALL
                    SELECT child.taxon_id
                    FROM taxa AS child
                    JOIN descendants ON child.parent_taxon_id = descendants.taxon_id
                )
                SELECT taxon_id FROM descendants
            ) AS selected_taxa
              ON selected_taxa.taxon_id = current_photo_taxon_mapping.taxon_id
            WHERE photos.photo_id > ?2
            ORDER BY photos.photo_id LIMIT ?3
        "#
        }
        (Some(_), false) => {
            r#"
            JOIN current_photo_taxon_mapping
              ON current_photo_taxon_mapping.photo_id = photos.photo_id
            WHERE current_photo_taxon_mapping.taxon_id = ?1
              AND photos.photo_id > ?2
            ORDER BY photos.photo_id LIMIT ?3
        "#
        }
        (None, _) => {
            r#"
            JOIN current_photo_taxon_mapping
              ON current_photo_taxon_mapping.photo_id = photos.photo_id
            WHERE photos.photo_id > ?2
            ORDER BY photos.photo_id LIMIT ?3
        "#
        }
    };
    let sql = photo_query(suffix);
    let mut statement = connection.prepare(&sql)?;
    let rows = match taxon_id {
        Some(taxon_id) => statement.query_map(
            params![taxon_id, after_photo_id, limit as i64],
            crate::db::photo_from_row,
        )?,
        None => statement.query_map(
            params![SqlValue::Null, after_photo_id, limit as i64],
            crate::db::photo_from_row,
        )?,
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn load_usage_taxon(
    connection: &rusqlite::Connection,
    taxon_id: i64,
    show_empty: bool,
) -> CoreResult<Option<PhotoTaxonUsage>> {
    connection
        .query_row(
            &format!(
                "{} WHERE taxa.taxon_id = ? AND (? OR COALESCE(photo_taxon_usage.subtree_photo_count, 0) > 0)",
                usage_taxon_select()
            ),
            params![taxon_id, show_empty],
            usage_taxon_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn load_usage_children_page(
    connection: &rusqlite::Connection,
    parent_taxon_id: Option<i64>,
    show_empty: bool,
    after_rank: i64,
    after_taxon_id: i64,
    limit: usize,
) -> CoreResult<Vec<PhotoTaxonUsage>> {
    let sql = format!(
        r#"
        {}
        WHERE ((?1 IS NULL AND taxa.parent_taxon_id IS NULL)
               OR taxa.parent_taxon_id = ?1)
          AND (?2 OR COALESCE(photo_taxon_usage.subtree_photo_count, 0) > 0)
          AND (?3 = 0 OR (taxa.rank, taxa.taxon_id) > (?4, ?5))
        ORDER BY taxa.rank, taxa.taxon_id
        LIMIT ?6
        "#,
        usage_taxon_select()
    );
    let has_cursor = after_rank != 0 || after_taxon_id != 0;
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            parent_taxon_id,
            show_empty,
            has_cursor,
            after_rank,
            after_taxon_id,
            limit as i64
        ],
        usage_taxon_from_row,
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn usage_taxon_select() -> &'static str {
    r#"
    SELECT taxa.taxon_id, taxa.rank,
           (SELECT name FROM taxon_names
            WHERE taxon_names.taxon_id = taxa.taxon_id
              AND name_type = 1) AS scientific_name,
           (SELECT name FROM taxon_names
            WHERE taxon_names.taxon_id = taxa.taxon_id
              AND name_type = 5) AS english_name,
           (SELECT name FROM taxon_names
            WHERE taxon_names.taxon_id = taxa.taxon_id
              AND name_type = 3) AS chinese_name,
           COALESCE(photo_taxon_usage.direct_photo_count, 0) AS direct_photo_count,
           COALESCE(photo_taxon_usage.subtree_photo_count, 0) AS subtree_photo_count
    FROM taxa
    LEFT JOIN current_photo_taxon_usage AS photo_taxon_usage USING (taxon_id)
    "#
}

fn usage_taxon_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PhotoTaxonUsage> {
    let rank = row.get::<_, i64>(1)?;
    Ok(PhotoTaxonUsage {
        taxon_id: row.get(0)?,
        rank: TaxonRank::from_code(rank).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        names: TaxonDisplayNames {
            sci_name: row.get(2)?,
            en_name: row.get(3)?,
            zh_name: row.get(4)?,
        },
        direct_photo_count: row.get(5)?,
        subtree_photo_count: row.get(6)?,
    })
}

fn photo_query(suffix: &str) -> String {
    format!(
        r#"
        SELECT photos.photo_id, photos.directory_id,
               CASE WHEN photo_directories.relative_path = '' THEN photos.filename
                    ELSE photo_directories.relative_path || '/' || photos.filename END AS relative_path,
               photos.filename, photos.file_size, photos.modified_at_ns, photos.thumbnail_path
        FROM photos
        JOIN photo_directories ON photo_directories.directory_id = photos.directory_id
        {suffix}
        "#
    )
}
