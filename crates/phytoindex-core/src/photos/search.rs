use rusqlite::params;

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
}
