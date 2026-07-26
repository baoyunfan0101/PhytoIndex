use rusqlite::{params, params_from_iter, types::Value as SqlValue};

use super::{
    PhotoCursor, decode_photo_cursor, encode_photo_cursor, invalid_photo_cursor, photo_page_limit,
};
use crate::models::{Photo, PhotoPage};
use crate::{CoreResult, Database};

pub fn search_photos_by_filename(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<Photo>> {
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
        Some(PhotoCursor::FilenameSearch {
            query: cursor_query,
            photo_id,
        }) if cursor_query == query => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let fetch_limit = limit + 1;
    let sql = if query.chars().count() >= 3 {
        super::photo_select(
            r#"
            JOIN photo_filenames_fts ON photo_filenames_fts.rowid = photos.photo_id
            WHERE photo_filenames_fts MATCH ?1
              AND photos.photo_id > ?2
            ORDER BY photos.photo_id
            LIMIT ?3
            "#,
        )
    } else {
        super::photo_select(
            r#"
            WHERE photos.filename LIKE ?1 ESCAPE '\'
              AND photos.photo_id > ?2
            ORDER BY photos.photo_id
            LIMIT ?3
            "#,
        )
    };
    let search_value = if query.chars().count() >= 3 {
        quoted_fts_match(&query)
    } else {
        format!("%{}%", escape_like(&query))
    };
    let connection = database.connect()?;
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![search_value, after_photo_id, fetch_limit as i64],
        crate::db::photo_from_row,
    )?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|photo| {
                encode_photo_cursor(&PhotoCursor::FilenameSearch {
                    query,
                    photo_id: photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

pub fn search_photos(
    database: &Database,
    query: &str,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<PhotoPage<Photo>> {
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
        Some(PhotoCursor::GeneralSearch {
            query: cursor_query,
            photo_id,
        }) if cursor_query == query => photo_id,
        Some(_) => return Err(invalid_photo_cursor()),
    };
    let limit = photo_page_limit(limit);
    let connection = database.connect()?;
    let taxonomy_relation = crate::taxonomy::taxon_search_relation(&connection, &query)?;
    let filename_match = if query.chars().count() >= 3 {
        "SELECT rowid FROM photo_filenames_fts WHERE photo_filenames_fts MATCH ?"
    } else {
        "SELECT photo_id FROM photos WHERE filename LIKE ? ESCAPE '\\'"
    };
    let filename_value = if query.chars().count() >= 3 {
        quoted_fts_match(&query)
    } else {
        format!("%{}%", escape_like(&query))
    };
    let mut parameters = taxonomy_relation.params;
    parameters.push(SqlValue::Text(filename_value));
    parameters.push(SqlValue::Integer(after_photo_id));
    let after_parameter = parameters.len();
    parameters.push(SqlValue::Integer(limit as i64 + 1));
    let limit_parameter = parameters.len();
    let sql = crate::photos::photo_select(&format!(
        r#"
        JOIN (
            WITH RECURSIVE {taxonomy_ctes},
            descendants(taxon_id) AS (
                SELECT taxon_id FROM ranked_taxa
                UNION
                SELECT taxa.taxon_id
                FROM taxa
                JOIN descendants
                  ON taxa.parent_taxon_id = descendants.taxon_id
            ),
            matched_photos(photo_id) AS (
                {filename_match}
                UNION
                SELECT photo_taxon_mapping.photo_id
                FROM photo_taxon_mapping
                JOIN descendants USING (taxon_id)
                WHERE photo_taxon_mapping.status = 'matched'
            )
            SELECT photo_id FROM matched_photos
        ) AS search_matches ON search_matches.photo_id = photos.photo_id
        WHERE photos.photo_id > ?{after_parameter}
        ORDER BY photos.photo_id
        LIMIT ?{limit_parameter}
        "#,
        taxonomy_ctes = taxonomy_relation.cte_sql,
    ));
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(parameters), crate::db::photo_from_row)?;
    let mut items = rows.collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if items.len() > limit {
        items.pop();
        items
            .last()
            .map(|photo| {
                encode_photo_cursor(&PhotoCursor::GeneralSearch {
                    query,
                    photo_id: photo.photo_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(PhotoPage { items, next_cursor })
}

fn quoted_fts_match(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
                    (1, 1, 'Canis lupus001.jpg', 1, 1),
                    (2, 1, 'Felis catus002.jpg', 1, 1),
                    (3, 1, 'canis familiaris003.jpg', 1, 1),
                    (4, 1, '100%field004.jpg', 1, 1);
                "#,
            )
            .unwrap();
        (directory, database)
    }

    #[test]
    fn filename_search_uses_scoped_cursor_pages() {
        let (_directory, database) = database();
        let first = search_photos_by_filename(&database, "CANIS", None, 1).unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|photo| photo.photo_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        let second =
            search_photos_by_filename(&database, "canis", first.next_cursor.as_deref(), 1).unwrap();
        assert_eq!(
            second
                .items
                .iter()
                .map(|photo| photo.photo_id)
                .collect::<Vec<_>>(),
            vec![3]
        );
        assert!(second.next_cursor.is_none());
        assert!(
            search_photos_by_filename(&database, "felis", first.next_cursor.as_deref(), 1).is_err()
        );
    }

    #[test]
    fn short_filename_search_escapes_like_wildcards() {
        let (_directory, database) = database();
        let page = search_photos_by_filename(&database, "%", None, 50).unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|photo| photo.photo_id)
                .collect::<Vec<_>>(),
            vec![4]
        );
    }

    #[test]
    fn general_search_merges_filename_and_taxon_results_without_duplicates() {
        let (_directory, database) = database();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO taxa (taxon_id, parent_taxon_id, rank) VALUES
                    (10, NULL, 3),
                    (11, 10, 4);
                INSERT INTO taxon_names (taxon_id, name_type, name) VALUES
                    (10, 1, 'Canidae'),
                    (11, 1, 'Canis');
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status) VALUES
                    (1, 11, 'matched'),
                    (2, 11, 'matched');
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES
                    (10, 0, 2),
                    (11, 2, 2);
                "#,
            )
            .unwrap();
        drop(connection);

        let first = search_photos(&database, "canis", None, 2).unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .map(|photo| photo.photo_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let second = search_photos(&database, "canis", first.next_cursor.as_deref(), 2).unwrap();
        assert_eq!(second.items[0].photo_id, 3);
        assert!(second.next_cursor.is_none());
        assert!(search_photos(&database, "felis", first.next_cursor.as_deref(), 2).is_err());
    }

    #[test]
    fn general_search_does_not_cap_matching_taxa() {
        let (_directory, database) = database();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TEMP TABLE test_sequence (
                    value INTEGER PRIMARY KEY
                );
                WITH digits(value) AS (
                    VALUES (0), (1), (2), (3), (4),
                           (5), (6), (7), (8), (9)
                )
                INSERT INTO test_sequence(value)
                SELECT ones.value
                     + tens.value * 10
                     + hundreds.value * 100
                     + thousands.value * 1000
                FROM digits AS ones
                CROSS JOIN digits AS tens
                CROSS JOIN digits AS hundreds
                CROSS JOIN digits AS thousands
                WHERE ones.value
                    + tens.value * 10
                    + hundreds.value * 100
                    + thousands.value * 1000
                    BETWEEN 1 AND 5001;

                INSERT INTO taxa (taxon_id, parent_taxon_id, rank)
                SELECT 20000 + value, NULL, 5
                FROM test_sequence;
                INSERT INTO taxon_names (taxon_id, name_type, name)
                SELECT 20000 + value, 1,
                       'Limitprobe ' || printf('%04d', value)
                FROM test_sequence;
                INSERT INTO photos (
                    photo_id, directory_id, filename, file_size, modified_at_ns
                )
                SELECT 10000 + value, 1,
                       'photo' || printf('%04d', value) || '.jpg', 1, 1
                FROM test_sequence;
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                SELECT 10000 + value, 20000 + value, 'matched'
                FROM test_sequence;
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                )
                SELECT 20000 + value, 1, 1
                FROM test_sequence;
                "#,
            )
            .unwrap();
        drop(connection);
        let cursor = encode_photo_cursor(&PhotoCursor::GeneralSearch {
            query: "limitprobe".into(),
            photo_id: 15000,
        })
        .unwrap();

        let page = search_photos(&database, "limitprobe", Some(&cursor), 1).unwrap();

        assert_eq!(page.items[0].photo_id, 15001);
    }
}
