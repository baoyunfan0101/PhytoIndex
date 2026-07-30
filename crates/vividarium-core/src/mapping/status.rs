use rusqlite::{params_from_iter, types::Value as SqlValue};

use super::{PhotoMappingListItem, PhotoMappingListStatus, PhotoMappingSummary, PhotoTaxonStatus};
use crate::models::PhotoPage;
use crate::photos::{
    self, PhotoCursor, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor,
    photo_page_limit,
};
use crate::{CoreResult, Database};

pub fn list_photos_by_mapping_status(
    database: &Database,
    status: PhotoMappingListStatus,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoMappingListItem>> {
    let connection = database.connect()?;
    let after_photo_id = match decode_photo_cursor(cursor)? {
        None => 0,
        Some(PhotoCursor::MappingStatus {
            status: cursor_status,
            photo_id,
        }) if cursor_status == status.as_str() => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let (joins, filter, values) = status_query_parts(status, after_photo_id, fetch_limit);
    let sql = photo_mapping_query(joins, filter);
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), |row| {
        mapping_list_item_from_row(row, status)
    })?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|item| {
                encode_photo_cursor(&PhotoCursor::MappingStatus {
                    status: status.as_str().into(),
                    photo_id: item.photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

pub fn search_photos_by_mapping_status(
    database: &Database,
    status: PhotoMappingListStatus,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<PhotoMappingListItem>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        if cursor.is_some_and(|value| !value.is_empty()) {
            return Err(invalid_photo_cursor());
        }
        return Ok(PhotoPage {
            items: Vec::new(),
            next_cursor: None,
        });
    }
    let after_photo_id = match decode_photo_cursor(cursor)? {
        None => 0,
        Some(PhotoCursor::MappingStatusSearch {
            status: cursor_status,
            query: cursor_query,
            photo_id,
        }) if cursor_status == status.as_str() && cursor_query == query => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let connection = database.connect()?;
    let search_relation = photos::photo_search_relation(
        &connection,
        &query,
        status == PhotoMappingListStatus::Matched,
    )?;
    let search_join = format!(
        r#"
        JOIN ({}) AS search_matches
          ON search_matches.photo_id = photos.photo_id
        "#,
        search_relation.sql
    );
    let (state_joins, filter, mut state_values) =
        status_query_parts(status, after_photo_id, fetch_limit);
    let mut parameters = search_relation.params;
    parameters.append(&mut state_values);
    let sql = photo_mapping_query(&format!("{search_join}{state_joins}"), filter);
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(parameters), |row| {
        mapping_list_item_from_row(row, status)
    })?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|item| {
                encode_photo_cursor(&PhotoCursor::MappingStatusSearch {
                    status: status.as_str().into(),
                    query,
                    photo_id: item.photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

fn status_query_parts(
    status: PhotoMappingListStatus,
    after_photo_id: i64,
    fetch_limit: usize,
) -> (&'static str, &'static str, Vec<SqlValue>) {
    match status {
        PhotoMappingListStatus::Processing => (
            r#"
            JOIN photo_mapping_queue ON photo_mapping_queue.photo_id = photos.photo_id
            LEFT JOIN photo_taxon_mapping ON photo_taxon_mapping.photo_id = photos.photo_id
            "#,
            "photos.photo_id > ?",
            vec![
                SqlValue::Integer(after_photo_id),
                SqlValue::Integer(fetch_limit as i64),
            ],
        ),
        status => (
            r#"
            LEFT JOIN photo_mapping_queue ON photo_mapping_queue.photo_id = photos.photo_id
            JOIN photo_taxon_mapping ON photo_taxon_mapping.photo_id = photos.photo_id
            "#,
            r#"
            photo_mapping_queue.photo_id IS NULL
            AND photo_taxon_mapping.status = ?
            AND photos.photo_id > ?
            "#,
            vec![
                SqlValue::Text(status.as_str().into()),
                SqlValue::Integer(after_photo_id),
                SqlValue::Integer(fetch_limit as i64),
            ],
        ),
    }
}

fn photo_mapping_query(joins: &str, filter: &str) -> String {
    format!(
        r#"
        SELECT photos.photo_id, photos.directory_id,
               CASE WHEN photo_directories.relative_path = '' THEN photos.filename
                    ELSE photo_directories.relative_path || '/' || photos.filename END AS relative_path,
               photos.filename, photos.file_size, photos.modified_at_ns, photos.thumbnail_path,
               photo_taxon_mapping.taxon_id AS mapping_taxon_id,
               photo_taxon_mapping.status AS mapping_status
        FROM photos
        JOIN photo_directories ON photo_directories.directory_id = photos.directory_id
        {joins}
        WHERE {filter}
        ORDER BY photos.photo_id
        LIMIT ?
        "#
    )
}

fn mapping_list_item_from_row(
    row: &rusqlite::Row<'_>,
    status: PhotoMappingListStatus,
) -> rusqlite::Result<PhotoMappingListItem> {
    let photo = crate::db::photo_from_row(row)?;
    let mapping = match status {
        PhotoMappingListStatus::Processing => PhotoMappingSummary {
            photo_id: photo.photo_id,
            taxon_id: None,
            status: PhotoTaxonStatus::Processing,
        },
        _ => {
            let stored_status = row.get::<_, String>("mapping_status")?;
            let stored_status = PhotoTaxonStatus::from_str(&stored_status).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            PhotoMappingSummary {
                photo_id: photo.photo_id,
                taxon_id: row.get("mapping_taxon_id")?,
                status: stored_status,
            }
        }
    };
    Ok(PhotoMappingListItem { photo, mapping })
}
