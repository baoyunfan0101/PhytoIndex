use std::collections::BTreeSet;
use std::io::Cursor;

use rusqlite::fallible_streaming_iterator::FallibleStreamingIterator;
use rusqlite::hooks::Action;
use rusqlite::session::{ChangesetItem, ChangesetIter, Session};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, params_from_iter};

use crate::{CoreError, CoreResult};

const TAXONOMY_SESSION_TABLES: [&str; 2] = ["taxa", "taxon_names"];

pub(super) fn is_taxonomy_session_table(table_name: &str) -> bool {
    TAXONOMY_SESSION_TABLES.contains(&table_name)
}

pub(super) fn start_taxonomy_session(connection: &Connection) -> CoreResult<Session<'_>> {
    let mut session = Session::new(connection)?;
    for table in TAXONOMY_SESSION_TABLES {
        session.attach(Some(table))?;
    }
    Ok(session)
}

pub(super) fn affected_taxon_ids_from_changeset(
    connection: &Connection,
    changeset_blob: &[u8],
) -> CoreResult<BTreeSet<i64>> {
    let input = &mut Cursor::new(changeset_blob) as &mut dyn std::io::Read;
    let mut changes = ChangesetIter::start_strm(&input)?;
    let mut taxon_ids = BTreeSet::new();
    let mut taxon_name_ids = BTreeSet::new();
    while let Some(item) = changes.next()? {
        let operation = item.op()?;
        match operation.table_name() {
            "taxa" => {
                collect_changeset_integers(item, operation.code(), 0, &mut taxon_ids)?;
                collect_changeset_integers(item, operation.code(), 1, &mut taxon_ids)?;
            }
            "taxon_names" => {
                if !collect_changeset_integers(item, operation.code(), 1, &mut taxon_ids)? {
                    collect_changeset_integers(item, operation.code(), 0, &mut taxon_name_ids)?;
                }
            }
            table => {
                return Err(CoreError::Consistency(format!(
                    "unexpected taxonomy changeset table: {table}"
                )));
            }
        }
    }
    drop(changes);
    for chunk in taxon_name_ids.into_iter().collect::<Vec<_>>().chunks(500) {
        let placeholders = std::iter::repeat_n("?", chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = connection.prepare(&format!(
            "SELECT DISTINCT taxon_id FROM taxon_names WHERE name_id IN ({placeholders})"
        ))?;
        for taxon_id in
            statement.query_map(params_from_iter(chunk.iter()), |row| row.get::<_, i64>(0))?
        {
            taxon_ids.insert(taxon_id?);
        }
    }
    Ok(taxon_ids)
}

fn collect_changeset_integers(
    item: &ChangesetItem,
    action: Action,
    column: usize,
    values: &mut BTreeSet<i64>,
) -> CoreResult<bool> {
    let mut found = false;
    match action {
        Action::SQLITE_INSERT => {
            found |= collect_changeset_integer(item.new_value(column), values)?;
        }
        Action::SQLITE_DELETE => {
            found |= collect_changeset_integer(item.old_value(column), values)?;
        }
        Action::SQLITE_UPDATE => {
            found |= collect_changeset_integer(item.old_value(column), values)?;
            found |= collect_changeset_integer(item.new_value(column), values)?;
        }
        _ => {
            return Err(CoreError::Consistency(format!(
                "unexpected taxonomy changeset action: {action:?}"
            )));
        }
    }
    Ok(found)
}

fn collect_changeset_integer(
    value: rusqlite::Result<ValueRef<'_>>,
    values: &mut BTreeSet<i64>,
) -> CoreResult<bool> {
    match value {
        Ok(ValueRef::Integer(value)) => {
            values.insert(value);
            Ok(true)
        }
        Ok(ValueRef::Null) => Ok(false),
        Err(rusqlite::Error::InvalidColumnIndex(_)) => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => Err(CoreError::Consistency(
            "taxonomy changeset identifier is not an integer".into(),
        )),
    }
}
