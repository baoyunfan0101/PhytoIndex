use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use super::formatted::{start_taxonomy_session, validate_taxonomy};
use super::view::load_taxon_summary;
use super::{
    TaxonInputRow, TaxonRank, TaxonRowStatus, TaxonomyNameType, TaxonomyOperationResult,
    apply_rows, preview_rows,
};
use crate::operations::{self, NewAuditRow, NewOperation};
use crate::{CoreError, CoreResult, Database};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonUpdateInput {
    pub taxon_id: i64,
    pub authority_year: Option<String>,
    pub synonyms: Vec<String>,
    pub zh_name: Option<String>,
    pub zh_alias: Vec<String>,
    pub en_name: Option<String>,
    pub en_alias: Vec<String>,
    pub geological_range: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeleteTaxonNameInput {
    pub taxon_id: i64,
    pub name_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromoteTaxonNameInput {
    pub taxon_id: i64,
    pub name_id: i64,
}

pub fn update_taxon(
    database: &Database,
    input: TaxonUpdateInput,
) -> CoreResult<TaxonomyOperationResult> {
    let connection = database.connect_taxonomy_metadata_context()?;
    let summary = load_taxon_summary(&connection, input.taxon_id)?
        .ok_or_else(|| CoreError::NotFound(format!("taxon {}", input.taxon_id)))?;
    drop(connection);
    let mut row = TaxonInputRow {
        selected_taxon_id: Some(input.taxon_id),
        authority_year: input.authority_year,
        synonyms: input.synonyms,
        zh_name: input.zh_name,
        zh_alias: input.zh_alias,
        en_name: input.en_name,
        en_alias: input.en_alias,
        geological_range: input.geological_range,
        source: input.source,
        ..TaxonInputRow::default()
    };
    for item in &summary.breadcrumb {
        set_rank_locator(&mut row, item.rank, item.names.sci_name.clone())?;
    }
    set_rank_locator(&mut row, summary.rank, summary.names.sci_name)?;
    let preview = preview_rows(database, std::slice::from_ref(&row))?;
    if preview.rows[0].operation_types.iter().any(|value| {
        matches!(
            value,
            TaxonRowStatus::Invalid
                | TaxonRowStatus::NotMatched
                | TaxonRowStatus::MultipleCandidates
        )
    }) {
        return Err(CoreError::InvalidArgument(preview.rows[0].message.clone()));
    }
    apply_rows(database, &[row])
}

pub fn promote_taxon_name(database: &Database, input: PromoteTaxonNameInput) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_metadata_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (rank, current_type, name) = transaction
        .query_row(
            r#"
            SELECT taxa.rank, taxon_names.name_type, taxon_names.name
            FROM taxa JOIN taxon_names USING (taxon_id)
            WHERE taxa.taxon_id = ? AND taxon_names.name_id = ?
            "#,
            params![input.taxon_id, input.name_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "name {} for taxon {}",
                input.name_id, input.taxon_id
            ))
        })?;
    let current_type = TaxonomyNameType::from_code(current_type)?;
    if current_type.is_primary() {
        return Err(CoreError::InvalidArgument(
            "the selected name is already accepted".into(),
        ));
    }
    let accepted_type = current_type.accepted_type();
    let (accepted_name_id, accepted_name) = transaction
        .query_row(
            "SELECT name_id, name FROM taxon_names WHERE taxon_id = ? AND name_type = ?",
            params![input.taxon_id, accepted_type.code()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::InvalidArgument(format!(
                "taxon {} has no {} to exchange",
                input.taxon_id,
                accepted_type.as_str()
            ))
        })?;
    if current_type == TaxonomyNameType::Synonym
        && TaxonRank::from_code(rank)? == TaxonRank::Species
    {
        let genus_name: Option<String> = transaction
            .query_row(
                r#"
                SELECT name
                FROM taxa AS species
                JOIN taxon_names
                  ON taxon_names.taxon_id = species.parent_taxon_id
                 AND taxon_names.name_type = 1
                WHERE species.taxon_id = ?
                "#,
                [input.taxon_id],
                |row| row.get(0),
            )
            .optional()?;
        let first_word = name.split_whitespace().next();
        if genus_name.as_deref() != first_word {
            return Err(CoreError::InvalidArgument(format!(
                "species scientific name '{}' does not start with parent genus '{}'",
                name,
                genus_name.unwrap_or_default()
            )));
        }
    }
    let mut session = start_taxonomy_session(&transaction)?;
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![current_type.code(), input.taxon_id, accepted_name_id],
    )?;
    transaction.execute(
        "UPDATE taxon_names SET name_type = ? WHERE taxon_id = ? AND name_id = ?",
        params![accepted_type.code(), input.taxon_id, input.name_id],
    )?;
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_name_promote",
            source: "ui_promote",
            total_items: 1,
            succeeded_items: 1,
            failed_items: 0,
            rollbackable: true,
            has_formatted_input: false,
        },
    )?;
    transaction.execute(
        r#"
        INSERT INTO operation_changesets (operation_id, changeset_blob)
        VALUES (?, ?)
        "#,
        params![operation_id, changeset_blob],
    )?;
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon_name",
            entity_id: Some(input.name_id.to_string()),
            action: "promote",
            before_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "accepted_name_id": accepted_name_id,
                "accepted_name": accepted_name.clone(),
                "promoted_name_id": input.name_id,
                "promoted_name": name.clone(),
                "accepted_name_type": accepted_type.code(),
                "alias_name_type": current_type.code(),
            })),
            after_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "accepted_name_id": input.name_id,
                "accepted_name": name,
                "alias_name_id": accepted_name_id,
                "alias_name": accepted_name,
                "accepted_name_type": accepted_type.code(),
                "alias_name_type": current_type.code(),
            })),
            succeeded: true,
            message: "exchanged accepted and alias taxonomy names",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [input.taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

pub fn delete_taxon_name(database: &Database, input: DeleteTaxonNameInput) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_metadata_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (name_type, name) = transaction
        .query_row(
            "SELECT name_type, name FROM taxon_names WHERE taxon_id = ? AND name_id = ?",
            params![input.taxon_id, input.name_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "name {} for taxon {}",
                input.name_id, input.taxon_id
            ))
        })?;
    if TaxonomyNameType::from_code(name_type)? == TaxonomyNameType::SciName {
        return Err(CoreError::InvalidArgument(
            "the unique sci_name cannot be deleted".into(),
        ));
    }
    let mut session = start_taxonomy_session(&transaction)?;
    let deleted = transaction.execute(
        "DELETE FROM taxon_names WHERE taxon_id = ? AND name_id = ?",
        params![input.taxon_id, input.name_id],
    )?;
    if deleted == 0 {
        return Err(CoreError::NotFound(format!(
            "name {} ('{}') for taxon {}",
            input.name_id, name, input.taxon_id
        )));
    }
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_delete",
            source: "ui_delete",
            total_items: 1,
            succeeded_items: 1,
            failed_items: 0,
            rollbackable: true,
            has_formatted_input: false,
        },
    )?;
    transaction.execute(
        r#"
        INSERT INTO operation_changesets (operation_id, changeset_blob)
        VALUES (?, ?)
        "#,
        params![operation_id, changeset_blob],
    )?;
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon_name",
            entity_id: Some(input.name_id.to_string()),
            action: "delete",
            before_json: Some(serde_json::json!({
                "taxon_id": input.taxon_id,
                "name_id": input.name_id,
                "name_type": name_type,
                "name": name,
            })),
            after_json: None,
            succeeded: true,
            message: "deleted taxonomy name",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [input.taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

pub fn delete_taxon(database: &Database, taxon_id: i64) -> CoreResult<()> {
    let mut connection = database.connect_taxonomy_metadata_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let taxon = transaction
        .query_row(
            r#"
        SELECT parent_taxon_id, rank, geological_range
        FROM taxa
        WHERE taxon_id = ?
        "#,
            [taxon_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("taxon {taxon_id}")))?;
    let child_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM taxa WHERE parent_taxon_id = ?",
        [taxon_id],
        |row| row.get(0),
    )?;
    if child_count > 0 {
        return Err(CoreError::InvalidArgument(format!(
            "taxon {taxon_id} cannot be deleted because it has child taxa"
        )));
    }
    let mut session = start_taxonomy_session(&transaction)?;
    transaction.execute("DELETE FROM taxa WHERE taxon_id = ?", [taxon_id])?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);
    let operation_id = operations::insert_operation(
        &transaction,
        NewOperation {
            kind: "taxonomy_delete",
            source: "ui_delete",
            total_items: 1,
            succeeded_items: 1,
            failed_items: 0,
            rollbackable: true,
            has_formatted_input: false,
        },
    )?;
    transaction.execute(
        r#"
        INSERT INTO operation_changesets (operation_id, changeset_blob)
        VALUES (?, ?)
        "#,
        params![operation_id, changeset_blob],
    )?;
    operations::insert_audit_row(
        &transaction,
        operation_id,
        NewAuditRow {
            sequence: 1,
            entity_type: "taxon",
            entity_id: Some(taxon_id.to_string()),
            action: "delete",
            before_json: Some(serde_json::json!({
                "taxon_id": taxon_id,
                "parent_taxon_id": taxon.0,
                "rank": taxon.1,
                "geological_range": taxon.2,
            })),
            after_json: None,
            succeeded: true,
            message: "deleted taxon",
        },
    )?;
    super::sync::record_event(&transaction, Some(operation_id), [taxon_id], false)?;
    transaction.commit()?;
    Ok(())
}

fn set_rank_locator(
    row: &mut TaxonInputRow,
    rank: TaxonRank,
    scientific_name: Option<String>,
) -> CoreResult<()> {
    let name = scientific_name.ok_or_else(|| {
        CoreError::InvalidArgument(format!("{} taxon has no sci_name", rank.as_str()))
    })?;
    match rank {
        TaxonRank::Kingdom => row.kingdom = Some(name),
        TaxonRank::Order => row.order = Some(name),
        TaxonRank::Family => row.family = Some(name),
        TaxonRank::Genus => row.genus = Some(name),
        TaxonRank::Species => row.species = Some(name),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn database() -> (TempDir, Database) {
        let directory = TempDir::new().unwrap();
        let database = Database::open_test(directory.path().join("test.db")).unwrap();
        (directory, database)
    }

    #[test]
    fn promoting_a_species_synonym_exchanges_name_types() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    order: Some("Carnivora".into()),
                    family: Some("Canidae".into()),
                    genus: Some("Canis".into()),
                    species: Some("Canis lupus".into()),
                    synonyms: vec!["Canis lycaon".into(), "Felis lupus".into()],
                    ..TaxonInputRow::default()
                },
            ],
        )
        .unwrap();
        let connection = database.connect_taxonomy_metadata_context().unwrap();
        let (taxon_id, promoted_name_id, invalid_name_id): (i64, i64, i64) = connection
            .query_row(
                r#"
                SELECT accepted.taxon_id, promoted.name_id, invalid.name_id
                FROM taxon_names AS accepted
                JOIN taxon_names AS promoted
                  ON promoted.taxon_id = accepted.taxon_id
                 AND promoted.name = 'Canis lycaon'
                JOIN taxon_names AS invalid
                  ON invalid.taxon_id = accepted.taxon_id
                 AND invalid.name = 'Felis lupus'
                WHERE accepted.name = 'Canis lupus'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        drop(connection);

        promote_taxon_name(
            &database,
            PromoteTaxonNameInput {
                taxon_id,
                name_id: promoted_name_id,
            },
        )
        .unwrap();
        let connection = database.connect_taxonomy_metadata_context().unwrap();
        let promoted: i64 = connection
            .query_row(
                "SELECT name_type FROM taxon_names WHERE name = 'Canis lycaon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let previous: i64 = connection
            .query_row(
                "SELECT name_type FROM taxon_names WHERE name = 'Canis lupus'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(promoted, TaxonomyNameType::SciName.code());
        assert_eq!(previous, TaxonomyNameType::Synonym.code());
        drop(connection);
        let operation = crate::taxonomy::list_operations(&database, None, 1)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(operation.kind, "taxonomy_name_promote");
        assert!(operation.rollbackable);
        assert!(!operation.has_formatted_input);

        let error = promote_taxon_name(
            &database,
            PromoteTaxonNameInput {
                taxon_id,
                name_id: invalid_name_id,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("parent genus"));

        crate::taxonomy::rollback_operation(&database, operation.operation_id).unwrap();
        let connection = database.connect_taxonomy_metadata_context().unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT name_type FROM taxon_names WHERE name = 'Canis lupus'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            TaxonomyNameType::SciName.code()
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT name_type FROM taxon_names WHERE name = 'Canis lycaon'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            TaxonomyNameType::Synonym.code()
        );
    }

    #[test]
    fn name_actions_require_matching_taxon_and_name_ids() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[
                TaxonInputRow {
                    kingdom: Some("Animalia".into()),
                    synonyms: vec!["Metazoa".into()],
                    ..TaxonInputRow::default()
                },
                TaxonInputRow {
                    kingdom: Some("Plantae".into()),
                    ..TaxonInputRow::default()
                },
            ],
        )
        .unwrap();
        let connection = database.connect_taxonomy_metadata_context().unwrap();
        let (animalia_id, sci_name_id, synonym_id): (i64, i64, i64) = connection
            .query_row(
                r#"
                SELECT sci.taxon_id, sci.name_id, synonym.name_id
                FROM taxon_names AS sci
                JOIN taxon_names AS synonym USING (taxon_id)
                WHERE sci.name = 'Animalia' AND synonym.name = 'Metazoa'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let plantae_id: i64 = connection
            .query_row(
                "SELECT taxon_id FROM taxon_names WHERE name = 'Plantae'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let detail = crate::taxonomy::get_taxon_detail(&database, animalia_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            detail.names.sci_name.as_ref().map(|name| name.name_id),
            Some(sci_name_id)
        );
        assert_eq!(detail.names.synonyms[0].name_id, synonym_id);

        let mismatch = delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: plantae_id,
                name_id: synonym_id,
            },
        )
        .unwrap_err();
        assert!(mismatch.to_string().contains("not found"));
        let sci_error = delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: animalia_id,
                name_id: sci_name_id,
            },
        )
        .unwrap_err();
        assert!(sci_error.to_string().contains("cannot be deleted"));

        delete_taxon_name(
            &database,
            DeleteTaxonNameInput {
                taxon_id: animalia_id,
                name_id: synonym_id,
            },
        )
        .unwrap();
        crate::taxonomy::synchronize_pending_photo_libraries(&database).unwrap();
        assert!(
            database
                .connect_taxonomy_metadata_context()
                .unwrap()
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM taxon_names WHERE name_id = ?)",
                    [synonym_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
        let operation = crate::taxonomy::list_operations(&database, None, 1)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(operation.kind, "taxonomy_delete");
        assert!(!operation.has_formatted_input);
        crate::taxonomy::rollback_operation(&database, operation.operation_id).unwrap();
        assert!(
            database
                .connect_taxonomy_metadata_context()
                .unwrap()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM taxon_names WHERE name_id = ?)",
                    [synonym_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap()
        );
    }

    #[test]
    fn custom_taxon_delete_queues_old_matches() {
        let (_directory, database) = database();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO taxa (rank) VALUES (5)", [])
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
        let name_id = connection.last_insert_rowid();
        connection
            .execute_batch(
                r#"
                INSERT INTO photo_directories (
                    directory_id, parent_directory_id, name, relative_path
                ) VALUES (1, NULL, '', '');
                INSERT INTO photos (
                    photo_id, directory_id, filename, file_size, modified_at_ns
                ) VALUES
                    (1, 1, 'Felis catus.jpg', 1, 1),
                    (2, 1, 'Ambiguous cat.jpg', 1, 1);
                "#,
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (1, ?, 'matched')
                "#,
                [taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_mapping (photo_id, taxon_id, status)
                VALUES (2, NULL, 'ambiguous')
                "#,
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO photo_taxon_candidates (photo_id, taxon_id) VALUES (2, ?)",
                [taxon_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_candidate_names (
                    photo_id, taxon_id, name_id, name_type, name
                ) VALUES (2, ?, ?, 1, 'Felis catus')
                "#,
                params![taxon_id, name_id],
            )
            .unwrap();
        connection
            .execute(
                r#"
                INSERT INTO photo_taxon_usage (
                    taxon_id, direct_photo_count, subtree_photo_count
                ) VALUES (?, 1, 1)
                "#,
                [taxon_id],
            )
            .unwrap();
        drop(connection);

        let result = crate::taxonomy::execute_custom_taxonomy_sql(
            &database,
            &crate::taxonomy::CustomTaxonomySqlRequest {
                sql: format!("DELETE FROM taxa WHERE taxon_id = {taxon_id}"),
                sources: Vec::new(),
                maximum_result_rows: None,
            },
        )
        .unwrap();
        crate::taxonomy::synchronize_pending_photo_libraries(&database).unwrap();
        assert!(
            crate::taxonomy::export_operation_input(&database, result.operation_id.unwrap())
                .unwrap_err()
                .to_string()
                .contains("formatted input")
        );

        let connection = database.connect().unwrap();
        let queued: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM photo_mapping_queue WHERE photo_id IN (1, 2)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 2);
    }
}
