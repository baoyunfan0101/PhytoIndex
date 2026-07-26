use rusqlite::params;

use super::{PhotoTaxonStatus, get_photo_mapping};
use crate::models::PhotoPage;
use crate::naming::normalize_taxonomy_name;
use crate::photos::{
    PhotoCursor, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor, photo_page_limit,
};
use crate::taxonomy::{
    TaxonSearchCursorKey, TaxonSearchResult, TaxonSuggestion,
    search_taxa_page_with_photos_connection, suggest_taxa_with_photos_connection,
};
use crate::{CoreError, CoreResult, Database};

pub fn search_photo_taxa(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<TaxonSearchResult>> {
    let Some(query) = normalize_taxonomy_name(query) else {
        if cursor.is_some_and(|value| !value.is_empty()) {
            return Err(invalid_photo_cursor());
        }
        return Ok(PhotoPage {
            items: Vec::new(),
            next_cursor: None,
        });
    };
    let after = match decode_photo_cursor(cursor)? {
        None => None,
        Some(PhotoCursor::TaxonSearch {
            query: cursor_query,
            match_level,
            edit_distance,
            sort_name,
            name_type_priority,
            taxon_id,
        }) if cursor_query == query => Some(TaxonSearchCursorKey {
            match_level,
            edit_distance,
            sort_name,
            name_type_priority,
            taxon_id,
        }),
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let connection = database.connect()?;
    let mut results =
        search_taxa_page_with_photos_connection(&connection, &query, after.as_ref(), limit + 1)?;
    let has_more = results.len() > limit;
    if has_more {
        results.pop();
    }
    let next_cursor = if has_more {
        results
            .last()
            .map(|result| {
                encode_photo_cursor(&PhotoCursor::TaxonSearch {
                    query,
                    match_level: result.key.match_level,
                    edit_distance: result.key.edit_distance,
                    sort_name: result.key.sort_name.clone(),
                    name_type_priority: result.key.name_type_priority,
                    taxon_id: result.key.taxon_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    let items = results.into_iter().map(|result| result.result).collect();
    Ok(PhotoPage { items, next_cursor })
}

pub fn get_photo_taxon_id(database: &Database, photo_id: i64) -> CoreResult<i64> {
    let mapping = get_photo_mapping(database, photo_id)?
        .ok_or_else(|| CoreError::NotFound(format!("photo {photo_id}")))?;
    if mapping.status != PhotoTaxonStatus::Matched {
        return Err(CoreError::InvalidArgument(format!(
            "photo {photo_id} does not have a current matched taxon"
        )));
    }
    mapping.taxon_id.ok_or_else(|| {
        CoreError::Consistency(format!("matched photo {photo_id} does not have a taxon ID"))
    })
}

pub fn suggest_photo_taxa(
    database: &Database,
    query: &str,
    limit: usize,
) -> CoreResult<Vec<TaxonSuggestion>> {
    suggest_taxa_with_photos_connection(&database.connect()?, query, limit)
}

pub fn list_taxon_photos(
    database: &Database,
    taxon_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<crate::models::Photo>> {
    let after_photo_id = match decode_photo_cursor(cursor)? {
        None => 0,
        Some(PhotoCursor::TaxonPhotos {
            taxon_id: cursor_taxon_id,
            photo_id,
        }) if cursor_taxon_id == taxon_id => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let connection = database.connect()?;
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM taxa WHERE taxon_id = ?)",
        [taxon_id],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(CoreError::NotFound(format!("taxon {taxon_id}")));
    }
    let limit = photo_page_limit(limit);
    let sql = crate::photos::photo_select(
        r#"
        JOIN current_photo_taxon_mapping
          ON current_photo_taxon_mapping.photo_id = photos.photo_id
        WHERE current_photo_taxon_mapping.photo_id > ?2
          AND current_photo_taxon_mapping.taxon_id IN (
              WITH RECURSIVE descendants(taxon_id) AS (
                  SELECT taxon_id FROM taxa WHERE taxon_id = ?1
                  UNION ALL
                  SELECT child.taxon_id
                  FROM taxa AS child
                  JOIN descendants
                    ON child.parent_taxon_id = descendants.taxon_id
              )
              SELECT taxon_id FROM descendants
          )
        ORDER BY photos.photo_id
        LIMIT ?3
        "#,
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![taxon_id, after_photo_id, limit as i64 + 1],
        crate::db::photo_from_row,
    )?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|photo| {
                encode_photo_cursor(&PhotoCursor::TaxonPhotos {
                    taxon_id,
                    photo_id: photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn database() -> (TempDir, Database) {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO photo_library (library_id, root_path)
                VALUES (1, '/photos');
                INSERT INTO photo_directories (
                    directory_id, parent_directory_id, name, relative_path
                ) VALUES (1, NULL, '', '');
                INSERT INTO photos (
                    photo_id, directory_id, filename, file_size, modified_at_ns
                ) VALUES
                    (1, 1, 'Canis001.jpg', 1, 1),
                    (2, 1, 'Felis002.jpg', 1, 1),
                    (3, 1, 'Unknown003.jpg', 1, 1);

                INSERT INTO taxa (taxon_id, parent_taxon_id, rank) VALUES
                    (10, NULL, 1),
                    (11, 10, 3),
                    (12, 10, 3),
                    (13, 10, 3);
                INSERT INTO taxon_names (
                    taxon_id, name_type, name
                ) VALUES
                    (10, 1, 'Animalia'),
                    (11, 1, 'Canidae'),
                    (12, 1, 'Felidae'),
                    (13, 1, 'Hominidae');
                INSERT INTO photo_taxon_mapping (
                    photo_id, taxon_id, status
                ) VALUES
                    (1, 11, 'matched'),
                    (2, 12, 'matched');
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES
                    (10, 0, 2),
                    (11, 1, 1),
                    (12, 1, 1);
                "#,
            )
            .unwrap();
        (directory, database)
    }

    #[test]
    fn taxonomy_search_filters_empty_taxa_and_pages_results() {
        let (_directory, database) = database();
        let first = search_photo_taxa(&database, "idae", None, 1).unwrap();
        assert_eq!(first.items[0].summary.taxon_id, 11);
        let second = search_photo_taxa(&database, "idae", first.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(second.items[0].summary.taxon_id, 12);
        assert!(second.next_cursor.is_none());
        assert!(search_photo_taxa(&database, "Animalia", first.next_cursor.as_deref(), 1).is_err());

        let ancestor = search_photo_taxa(&database, "Animalia", None, 50).unwrap();
        assert_eq!(ancestor.items.len(), 1);
        assert_eq!(ancestor.items[0].summary.taxon_id, 10);

        let suggestions = suggest_photo_taxa(&database, "idae", 10).unwrap();
        assert_eq!(
            suggestions
                .iter()
                .map(|suggestion| suggestion.taxon_id)
                .collect::<Vec<_>>(),
            vec![11, 12]
        );
        assert_eq!(suggestions[0].names.sci_name.as_deref(), Some("Canidae"));
        assert_eq!(suggestions[0].matches[0].name, "Canidae");
    }

    #[test]
    fn photo_and_taxon_navigation_use_current_mapping_tree() {
        let (_directory, database) = database();
        assert_eq!(get_photo_taxon_id(&database, 1).unwrap(), 11);
        assert!(get_photo_taxon_id(&database, 3).is_err());
        assert!(get_photo_taxon_id(&database, 999).is_err());
        let initial = list_taxon_photos(&database, 10, None, 1).unwrap();
        assert_eq!(initial.items[0].photo_id, 1);
        assert!(initial.next_cursor.is_some());

        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO photo_mapping_queue (photo_id, reason) VALUES (1, 'refresh')",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(get_photo_taxon_id(&database, 1).is_err());

        let current = list_taxon_photos(&database, 10, None, 1).unwrap();
        assert_eq!(current.items[0].photo_id, 2);
        assert!(current.next_cursor.is_none());
        assert!(
            list_taxon_photos(&database, 11, None, 1)
                .unwrap()
                .items
                .is_empty()
        );
        assert!(list_taxon_photos(&database, 11, initial.next_cursor.as_deref(), 1).is_err());
        assert!(
            search_photo_taxa(&database, "Canidae", None, 10)
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            suggest_photo_taxa(&database, "Canidae", 10)
                .unwrap()
                .is_empty()
        );
        assert!(list_taxon_photos(&database, 999, None, 1).is_err());
    }
}
