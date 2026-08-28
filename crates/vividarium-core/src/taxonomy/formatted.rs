//! Formatted taxonomy input, preview, apply, operation history, and rollback.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Cursor;

use csv::{ReaderBuilder, WriterBuilder};
use rusqlite::fallible_streaming_iterator::FallibleStreamingIterator;
use rusqlite::hooks::Action;
use rusqlite::session::{
    ChangesetItem, ChangesetIter, ConflictAction, ConflictType, Session, invert_strm,
};
use rusqlite::types::ValueRef;
use rusqlite::{
    Connection, OptionalExtension, Transaction, TransactionBehavior, params, params_from_iter,
};
use serde::{Deserialize, Serialize};

use super::match_exact_taxonomy_name;
use super::view::{TaxonSummary, load_taxon_summaries, load_taxon_summary};
use crate::models::{OperationProgress, OperationProgressUnit};
use crate::naming::{SynonymAuthorityParser, normalize_taxonomy_name};
use crate::operations::{
    self, NewAuditRow, NewOperation, OperationAuditRow, OperationPage, OperationSummary,
};
use crate::{CancellationToken, CoreError, CoreResult, Database};

pub const TAXONOMY_INPUT_COLUMNS: [&str; 13] = [
    "kingdom",
    "order",
    "family",
    "genus",
    "species",
    "authority_year",
    "synonyms",
    "zh_name",
    "zh_alias",
    "en_name",
    "en_alias",
    "geological_range",
    "source",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonRank {
    Kingdom,
    Order,
    Family,
    Genus,
    Species,
}

impl TaxonRank {
    pub(crate) const ALL: [Self; 5] = [
        Self::Kingdom,
        Self::Order,
        Self::Family,
        Self::Genus,
        Self::Species,
    ];

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Kingdom => "kingdom",
            Self::Order => "order",
            Self::Family => "family",
            Self::Genus => "genus",
            Self::Species => "species",
        }
    }

    pub(crate) fn code(self) -> i64 {
        self.index() as i64 + 1
    }

    pub(crate) fn from_code(value: i64) -> CoreResult<Self> {
        Self::ALL
            .get(value.saturating_sub(1) as usize)
            .copied()
            .ok_or_else(|| CoreError::InvalidArgument(format!("invalid taxon rank code: {value}")))
    }

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Kingdom => 0,
            Self::Order => 1,
            Self::Family => 2,
            Self::Genus => 3,
            Self::Species => 4,
        }
    }

    fn parent(self) -> Option<Self> {
        Self::ALL.get(self.index().wrapping_sub(1)).copied()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyNameType {
    SciName,
    Synonym,
    ZhName,
    ZhAlias,
    EnName,
    EnAlias,
}

impl TaxonomyNameType {
    pub(crate) const ALL: [Self; 6] = [
        Self::SciName,
        Self::Synonym,
        Self::ZhName,
        Self::ZhAlias,
        Self::EnName,
        Self::EnAlias,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::SciName => "sci_name",
            Self::Synonym => "synonym",
            Self::ZhName => "zh_name",
            Self::ZhAlias => "zh_alias",
            Self::EnName => "en_name",
            Self::EnAlias => "en_alias",
        }
    }

    pub fn from_value(value: &str) -> CoreResult<Self> {
        match value {
            "sci_name" => Ok(Self::SciName),
            "synonym" => Ok(Self::Synonym),
            "zh_name" => Ok(Self::ZhName),
            "zh_alias" => Ok(Self::ZhAlias),
            "en_name" => Ok(Self::EnName),
            "en_alias" => Ok(Self::EnAlias),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid taxonomy name type: {value}"
            ))),
        }
    }

    pub(crate) fn code(self) -> i64 {
        self.index() as i64 + 1
    }

    pub(crate) fn from_code(value: i64) -> CoreResult<Self> {
        Self::ALL
            .get(value.saturating_sub(1) as usize)
            .copied()
            .ok_or_else(|| {
                CoreError::InvalidArgument(format!("invalid taxonomy name type code: {value}"))
            })
    }

    fn index(self) -> usize {
        match self {
            Self::SciName => 0,
            Self::Synonym => 1,
            Self::ZhName => 2,
            Self::ZhAlias => 3,
            Self::EnName => 4,
            Self::EnAlias => 5,
        }
    }

    pub fn is_primary(self) -> bool {
        matches!(self, Self::SciName | Self::ZhName | Self::EnName)
    }

    pub fn accepted_type(self) -> Self {
        match self {
            Self::SciName | Self::Synonym => Self::SciName,
            Self::ZhName | Self::ZhAlias => Self::ZhName,
            Self::EnName | Self::EnAlias => Self::EnName,
        }
    }

    pub fn alias_type(self) -> Self {
        match self {
            Self::SciName | Self::Synonym => Self::Synonym,
            Self::ZhName | Self::ZhAlias => Self::ZhAlias,
            Self::EnName | Self::EnAlias => Self::EnAlias,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TaxonInputRow {
    pub kingdom: Option<String>,
    pub order: Option<String>,
    pub family: Option<String>,
    pub genus: Option<String>,
    pub species: Option<String>,
    pub authority_year: Option<String>,
    pub synonyms: Vec<String>,
    pub zh_name: Option<String>,
    pub zh_alias: Vec<String>,
    pub en_name: Option<String>,
    pub en_alias: Vec<String>,
    pub geological_range: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonRowStatus {
    NoChange,
    Supplement,
    NewName,
    NewTaxon,
    Overwrite,
    Invalid,
    NotMatched,
    MultipleCandidates,
}

impl TaxonRowStatus {
    fn is_failure(self) -> bool {
        matches!(
            self,
            Self::Invalid | Self::NotMatched | Self::MultipleCandidates
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonChangeKind {
    CreateTaxon,
    AppendName,
    Supplement,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonChange {
    pub kind: TaxonChangeKind,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonRowOutcome {
    pub row_number: usize,
    pub operation_types: Vec<TaxonRowStatus>,
    pub message: String,
    pub target: Option<TaxonSummary>,
    pub parent: Option<TaxonSummary>,
    pub candidates: Vec<TaxonSummary>,
    pub changes: Vec<TaxonChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyPreviewResult {
    pub delimiter: String,
    pub encoding: String,
    pub rows: Vec<TaxonRowOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyOperationResult {
    pub operation_id: i64,
    pub total_rows: usize,
    pub succeeded_rows: usize,
    pub failed_rows: usize,
    pub delimiter: String,
    pub encoding: String,
    pub rows: Vec<TaxonRowOutcome>,
}

#[derive(Debug)]
pub struct PreparedTaxonomyUpdate {
    rows: Vec<TaxonInputRow>,
    preview: TaxonomyPreviewResult,
    changeset_blob: Vec<u8>,
    revision: TaxonomyRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaxonomyRevision {
    taxonomy_identity: String,
    latest_operation_id: i64,
    operation_count: i64,
}

impl PreparedTaxonomyUpdate {
    pub fn preview_result(&self) -> &TaxonomyPreviewResult {
        &self.preview
    }
}

pub fn preview_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyPreviewResult> {
    Ok(prepare_rows(database, rows)?.preview)
}

pub fn prepare_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<PreparedTaxonomyUpdate> {
    prepare_rows_with_cancellation(database, rows, &CancellationToken::new())
}

pub fn prepare_rows_with_cancellation(
    database: &Database,
    rows: &[TaxonInputRow],
    cancellation: &CancellationToken,
) -> CoreResult<PreparedTaxonomyUpdate> {
    cancellation.check()?;
    let delimiter = crate::general::get_csv_delimiter(database)?;
    let _guard = database.try_taxonomy_mutation()?;
    let mut connection = database.connect_taxonomy_metadata_context()?;
    cancellation.install_sqlite_progress_handler(&connection);
    let result = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision = taxonomy_revision(&transaction)?;
        let mut session = start_taxonomy_session(&transaction)?;
        let outcomes = process_rows(&transaction, rows, cancellation)?;
        validate_taxonomy(&transaction)?;
        cancellation.check()?;
        let mut changeset_blob = Vec::new();
        session.changeset_strm(&mut changeset_blob)?;
        drop(session);
        transaction.rollback()?;
        Ok(PreparedTaxonomyUpdate {
            rows: rows.to_vec(),
            preview: TaxonomyPreviewResult {
                delimiter,
                encoding: "UTF-8".into(),
                rows: outcomes,
            },
            changeset_blob,
            revision,
        })
    })();
    cancellation.normalize(result)
}

pub fn apply_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyOperationResult> {
    let delimiter = crate::general::get_csv_delimiter(database)?;
    let _guard = database.try_taxonomy_mutation()?;
    let mut connection = database.connect_taxonomy_metadata_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut session = start_taxonomy_session(&transaction)?;
    let outcomes = process_rows(&transaction, rows, &CancellationToken::new())?;
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);

    let result = store_applied_rows(&transaction, rows, outcomes, changeset_blob, delimiter)?;
    transaction.commit()?;
    Ok(result)
}

pub fn apply_prepared_rows(
    database: &Database,
    prepared: PreparedTaxonomyUpdate,
) -> CoreResult<TaxonomyOperationResult> {
    apply_prepared_rows_with_cancellation(database, prepared, &CancellationToken::new())
}

pub fn apply_prepared_rows_with_cancellation(
    database: &Database,
    prepared: PreparedTaxonomyUpdate,
    cancellation: &CancellationToken,
) -> CoreResult<TaxonomyOperationResult> {
    cancellation.check()?;
    let delimiter = prepared.preview.delimiter.clone();
    let _guard = database.try_taxonomy_mutation()?;
    let mut connection = database.connect_taxonomy_metadata_context()?;
    cancellation.install_sqlite_progress_handler(&connection);
    let result = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if taxonomy_revision(&transaction)? != prepared.revision {
            return Err(CoreError::InvalidArgument(
                "formatted update preview is stale; preview again".into(),
            ));
        }
        if !prepared.changeset_blob.is_empty() {
            transaction
                .apply_strm(
                    &mut Cursor::new(&prepared.changeset_blob),
                    Some(is_taxonomy_session_table),
                    |_, _| ConflictAction::SQLITE_CHANGESET_ABORT,
                )
                .map_err(|_| {
                    CoreError::InvalidArgument(
                        "formatted update preview is stale; preview again".into(),
                    )
                })?;
        }
        validate_taxonomy(&transaction)?;
        let result = store_applied_rows(
            &transaction,
            &prepared.rows,
            prepared.preview.rows,
            prepared.changeset_blob,
            delimiter,
        )?;
        cancellation.check()?;
        transaction.commit()?;
        Ok(result)
    })();
    cancellation.normalize(result)
}

fn store_applied_rows(
    transaction: &Transaction<'_>,
    rows: &[TaxonInputRow],
    outcomes: Vec<TaxonRowOutcome>,
    changeset_blob: Vec<u8>,
    delimiter: String,
) -> CoreResult<TaxonomyOperationResult> {
    let failed_rows = outcomes
        .iter()
        .filter(|row| row.operation_types.iter().any(|value| value.is_failure()))
        .count();
    let operation_id = operations::insert_operation(
        transaction,
        NewOperation {
            kind: "taxonomy_update",
            source: "formatted_update",
            total_items: outcomes.len(),
            succeeded_items: outcomes.len() - failed_rows,
            failed_items: failed_rows,
            rollbackable: true,
            has_formatted_input: true,
        },
    )?;
    let result = TaxonomyOperationResult {
        operation_id,
        total_rows: outcomes.len(),
        succeeded_rows: outcomes.len() - failed_rows,
        failed_rows,
        delimiter,
        encoding: "UTF-8".into(),
        rows: outcomes,
    };
    transaction.execute(
        r#"
        INSERT INTO operation_changesets (operation_id, changeset_blob)
        VALUES (?, ?)
        "#,
        params![operation_id, changeset_blob],
    )?;
    let mut insert_input = transaction.prepare_cached(
        r#"
        INSERT INTO operation_formatted_inputs (
            operation_id, sequence, input_json
        ) VALUES (?, ?, ?)
        "#,
    )?;
    for (index, input) in rows.iter().enumerate() {
        insert_input.execute(params![
            operation_id,
            (index + 1) as i64,
            serialize_json(input, "taxonomy operation input")?
        ])?;
    }
    drop(insert_input);
    insert_operation_audit(transaction, operation_id, &result.rows)?;
    let affected_taxon_ids = affected_taxon_ids_from_changeset(transaction, &changeset_blob)?;
    super::sync::record_event(transaction, Some(operation_id), affected_taxon_ids, false)?;
    Ok(result)
}

fn taxonomy_revision(connection: &Connection) -> CoreResult<TaxonomyRevision> {
    connection
        .query_row(
            r#"
            SELECT taxonomy_identity,
                   (SELECT COALESCE(MAX(operation_id), 0) FROM operations),
                   (SELECT COUNT(*) FROM operations)
            FROM taxonomy_identity
            WHERE identity_id = 1
            "#,
            [],
            |row| {
                Ok(TaxonomyRevision {
                    taxonomy_identity: row.get(0)?,
                    latest_operation_id: row.get(1)?,
                    operation_count: row.get(2)?,
                })
            },
        )
        .map_err(Into::into)
}

fn insert_operation_audit(
    transaction: &Transaction<'_>,
    operation_id: i64,
    outcomes: &[TaxonRowOutcome],
) -> CoreResult<()> {
    for outcome in outcomes {
        let succeeded = !outcome
            .operation_types
            .iter()
            .any(|status| status.is_failure());
        let before = outcome
            .changes
            .iter()
            .map(|change| {
                serde_json::json!({
                    "field": change.field,
                    "value": change.old_value,
                })
            })
            .collect::<Vec<_>>();
        let after = outcome
            .changes
            .iter()
            .map(|change| {
                serde_json::json!({
                    "field": change.field,
                    "value": change.new_value,
                })
            })
            .collect::<Vec<_>>();
        operations::insert_audit_row(
            transaction,
            operation_id,
            NewAuditRow {
                sequence: outcome.row_number,
                entity_type: "taxon",
                entity_id: outcome
                    .target
                    .as_ref()
                    .map(|target| target.taxon_id.to_string()),
                action: "formatted_update",
                before_json: Some(serde_json::json!({ "fields": before })),
                after_json: Some(serde_json::json!({
                    "operation_types": outcome.operation_types,
                    "fields": after,
                })),
                succeeded,
                message: &outcome.message,
            },
        )?;
    }
    Ok(())
}

fn process_rows(
    transaction: &Transaction<'_>,
    rows: &[TaxonInputRow],
    cancellation: &CancellationToken,
) -> CoreResult<Vec<TaxonRowOutcome>> {
    let synonym_parser = SynonymAuthorityParser::load(transaction)?;
    let mut outcomes = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        cancellation.check()?;
        outcomes.push(process_row(transaction, &synonym_parser, index + 1, row)?);
    }
    Ok(outcomes)
}

fn process_row(
    transaction: &Transaction<'_>,
    synonym_parser: &SynonymAuthorityParser,
    row_number: usize,
    row: &TaxonInputRow,
) -> CoreResult<TaxonRowOutcome> {
    let normalized = match NormalizedInput::from_row(row, synonym_parser) {
        Ok(value) => value,
        Err(message) => return Ok(failed_outcome(row_number, TaxonRowStatus::Invalid, message)),
    };
    let match_result = find_target(transaction, &normalized)?;

    match match_result {
        MatchResult::Many(candidates) => Ok(TaxonRowOutcome {
            row_number,
            operation_types: vec![TaxonRowStatus::MultipleCandidates],
            message: candidate_message(&candidates),
            target: None,
            parent: None,
            candidates,
            changes: Vec::new(),
        }),
        MatchResult::None => create_taxon(transaction, row_number, &normalized),
        MatchResult::One(summary, matched_name) => {
            update_existing(transaction, row_number, &normalized, summary, &matched_name)
        }
    }
}

fn create_taxon(
    transaction: &Transaction<'_>,
    row_number: usize,
    input: &NormalizedInput,
) -> CoreResult<TaxonRowOutcome> {
    let mut changes = Vec::new();
    let parent = if let Some(parent_rank) = input.target_rank.parent() {
        match resolve_or_create_lineage(
            transaction,
            input,
            parent_rank,
            input.target_rank,
            &mut changes,
        )? {
            Ok(parent) => Some(parent),
            Err(LineageFailure::MissingParent {
                child_rank,
                parent_rank,
            }) => {
                return Ok(failed_outcome(
                    row_number,
                    TaxonRowStatus::NotMatched,
                    format!(
                        "new {} taxon requires a {} scientific name",
                        child_rank.as_str(),
                        parent_rank.as_str()
                    ),
                ));
            }
            Err(LineageFailure::MultipleCandidates(candidates)) => {
                return Ok(TaxonRowOutcome {
                    row_number,
                    operation_types: vec![TaxonRowStatus::MultipleCandidates],
                    message: candidate_message(&candidates),
                    target: None,
                    parent: None,
                    candidates,
                    changes: Vec::new(),
                });
            }
        }
    } else {
        None
    };

    let target = insert_taxon_with_scientific_name(
        transaction,
        input.target_rank,
        &input.target_name,
        parent.as_ref(),
        input.geological_range.as_deref(),
        input.authority_year.as_deref(),
        input.source.as_deref(),
        &mut changes,
    )?;
    let taxon_id = target.taxon_id;
    apply_additional_names(transaction, taxon_id, input, &mut changes)?;
    let mut operation_types = classify_changes(&changes);
    operation_types.insert(0, TaxonRowStatus::NewTaxon);
    Ok(TaxonRowOutcome {
        row_number,
        operation_types,
        message: describe_changes(&changes),
        target: Some(target),
        parent,
        candidates: Vec::new(),
        changes,
    })
}

fn update_existing(
    transaction: &Transaction<'_>,
    row_number: usize,
    input: &NormalizedInput,
    summary: TaxonSummary,
    matched_name: &MatchedName,
) -> CoreResult<TaxonRowOutcome> {
    let taxon_id = summary.taxon_id;
    let mut changes = Vec::new();
    update_taxon_field(
        transaction,
        taxon_id,
        "geological_range",
        input.geological_range.as_deref(),
        &mut changes,
    )?;
    update_name_fields(
        transaction,
        taxon_id,
        matched_name.existing_type,
        &matched_name.name,
        matched_name.authority_year.as_deref(),
        input.source.as_deref(),
        &mut changes,
    )?;
    for (index, name) in input.scientific_names().into_iter().enumerate() {
        if index == matched_name.input_index {
            continue;
        }
        add_or_supplement_name(
            transaction,
            taxon_id,
            TaxonomyNameType::Synonym,
            &name.name,
            name.authority_year.as_deref(),
            input.source.as_deref(),
            &mut changes,
        )?;
    }
    apply_localized_input_names(transaction, taxon_id, input, &mut changes)?;
    let operation_types = classify_changes(&changes);
    let target = load_taxon_summary(transaction, taxon_id)?;
    Ok(TaxonRowOutcome {
        row_number,
        operation_types,
        message: if changes.is_empty() {
            "input produces no change".into()
        } else {
            describe_changes(&changes)
        },
        target,
        parent: None,
        candidates: Vec::new(),
        changes,
    })
}

fn apply_additional_names(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    input: &NormalizedInput,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    for synonym in &input.synonyms {
        add_or_supplement_name(
            transaction,
            taxon_id,
            TaxonomyNameType::Synonym,
            &synonym.name,
            synonym.authority_year.as_deref(),
            input.source.as_deref(),
            changes,
        )?;
    }
    apply_localized_input_names(transaction, taxon_id, input, changes)
}

fn apply_localized_input_names(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    input: &NormalizedInput,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    apply_localized_names(
        transaction,
        taxon_id,
        TaxonomyNameType::ZhName,
        TaxonomyNameType::ZhAlias,
        &input.zh_names,
        input.source.as_deref(),
        changes,
    )?;
    apply_localized_names(
        transaction,
        taxon_id,
        TaxonomyNameType::EnName,
        TaxonomyNameType::EnAlias,
        &input.en_names,
        input.source.as_deref(),
        changes,
    )
}

fn apply_localized_names(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    accepted_type: TaxonomyNameType,
    alias_type: TaxonomyNameType,
    names: &[String],
    source: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    let mut has_accepted: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM taxon_names WHERE taxon_id = ? AND name_type = ?)",
        params![taxon_id, accepted_type.code()],
        |row| row.get(0),
    )?;
    for name in names {
        let existing_type = existing_name_type(transaction, taxon_id, name, accepted_type)?;
        if let Some(existing_type) = existing_type {
            update_name_fields(
                transaction,
                taxon_id,
                existing_type,
                name,
                None,
                source,
                changes,
            )?;
            continue;
        }
        let name_type = if has_accepted {
            alias_type
        } else {
            has_accepted = true;
            accepted_type
        };
        insert_name(
            transaction,
            taxon_id,
            name_type,
            name,
            None,
            source,
            changes,
        )?;
    }
    Ok(())
}

fn add_or_supplement_name(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    requested_type: TaxonomyNameType,
    name: &str,
    authority_year: Option<&str>,
    source: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    if let Some(existing_type) = existing_name_type(transaction, taxon_id, name, requested_type)? {
        update_name_fields(
            transaction,
            taxon_id,
            existing_type,
            name,
            authority_year,
            source,
            changes,
        )
    } else {
        insert_name(
            transaction,
            taxon_id,
            requested_type,
            name,
            authority_year,
            source,
            changes,
        )
    }
}

fn insert_name(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    name_type: TaxonomyNameType,
    name: &str,
    authority_year: Option<&str>,
    source: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    transaction.execute(
        r#"
        INSERT INTO taxon_names (taxon_id, name_type, name, authority_year, source)
        VALUES (?, ?, ?, ?, ?)
        "#,
        params![taxon_id, name_type.code(), name, authority_year, source],
    )?;
    changes.push(TaxonChange {
        kind: TaxonChangeKind::AppendName,
        field: name_type.as_str().into(),
        old_value: None,
        new_value: Some(name.into()),
    });
    Ok(())
}

fn update_taxon_field(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    field: &str,
    new_value: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    let Some(new_value) = new_value else {
        return Ok(());
    };
    let old_value: Option<String> = transaction.query_row(
        "SELECT geological_range FROM taxa WHERE taxon_id = ?",
        [taxon_id],
        |row| row.get(0),
    )?;
    if old_value.as_deref() == Some(new_value) {
        return Ok(());
    }
    transaction.execute(
        "UPDATE taxa SET geological_range = ? WHERE taxon_id = ?",
        params![new_value, taxon_id],
    )?;
    changes.push(TaxonChange {
        kind: if old_value.is_some() {
            TaxonChangeKind::Overwrite
        } else {
            TaxonChangeKind::Supplement
        },
        field: format!("taxa.{field}"),
        old_value,
        new_value: Some(new_value.into()),
    });
    Ok(())
}

fn update_name_fields(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    name_type: TaxonomyNameType,
    name: &str,
    authority_year: Option<&str>,
    source: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    let current = transaction
        .query_row(
            r#"
            SELECT authority_year, source
            FROM taxon_names
            WHERE taxon_id = ? AND name_type = ? AND name = ?
            "#,
            params![taxon_id, name_type.code(), name],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()?;
    let Some((old_authority, old_source)) = current else {
        return Ok(());
    };
    if let Some(authority_year) = authority_year
        && old_authority.as_deref() != Some(authority_year)
    {
        transaction.execute(
            r#"
            UPDATE taxon_names SET authority_year = ?
            WHERE taxon_id = ? AND name_type = ? AND name = ?
            "#,
            params![authority_year, taxon_id, name_type.code(), name],
        )?;
        changes.push(TaxonChange {
            kind: if old_authority.is_some() {
                TaxonChangeKind::Overwrite
            } else {
                TaxonChangeKind::Supplement
            },
            field: format!("{}.authority_year", name_type.as_str()),
            old_value: old_authority,
            new_value: Some(authority_year.into()),
        });
    }
    if old_source.is_none()
        && let Some(source) = source
    {
        transaction.execute(
            r#"
            UPDATE taxon_names SET source = ?
            WHERE taxon_id = ? AND name_type = ? AND name = ?
            "#,
            params![source, taxon_id, name_type.code(), name],
        )?;
        changes.push(TaxonChange {
            kind: TaxonChangeKind::Supplement,
            field: format!("{}.source", name_type.as_str()),
            old_value: None,
            new_value: Some(source.into()),
        });
    }
    Ok(())
}

fn existing_name_type(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    name: &str,
    family: TaxonomyNameType,
) -> CoreResult<Option<TaxonomyNameType>> {
    let accepted_type = family.accepted_type();
    let alias_type = family.alias_type();
    transaction
        .query_row(
            r#"
            SELECT name_type
            FROM taxon_names
            WHERE taxon_id = ?
              AND name = ? COLLATE BINARY
              AND name_type IN (?, ?)
            "#,
            params![taxon_id, name, accepted_type.code(), alias_type.code()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(TaxonomyNameType::from_code)
        .transpose()
}

fn find_target(transaction: &Transaction<'_>, input: &NormalizedInput) -> CoreResult<MatchResult> {
    for (input_index, input_name) in input.scientific_names().into_iter().enumerate() {
        let matches =
            find_preferred_scientific_matches(transaction, input.target_rank, &input_name.name)?;
        if matches.is_empty() {
            continue;
        }
        let mut candidates = load_scientific_candidates(transaction, matches)?;
        if candidates.len() > 1 {
            candidates =
                disambiguate_candidates(transaction, candidates, input, input.target_rank)?;
        }
        if let [candidate] = candidates.as_slice() {
            return Ok(MatchResult::One(
                candidate.summary.clone(),
                MatchedName {
                    input_index,
                    name: input_name.name,
                    authority_year: input_name.authority_year,
                    existing_type: candidate.existing_type,
                },
            ));
        }
        if candidates.len() > 1 {
            return Ok(MatchResult::Many(
                candidates
                    .into_iter()
                    .map(|candidate| candidate.summary)
                    .collect(),
            ));
        }
    }
    Ok(MatchResult::None)
}

fn find_preferred_scientific_matches(
    transaction: &Transaction<'_>,
    rank: TaxonRank,
    name: &str,
) -> CoreResult<Vec<ScientificMatch>> {
    let mut matches =
        match_exact_taxonomy_name(transaction, name, rank, TaxonomyNameType::SciName)?;
    if matches.is_empty() {
        matches = match_exact_taxonomy_name(transaction, name, rank, TaxonomyNameType::Synonym)?;
    }
    Ok(matches
        .into_iter()
        .map(|matched| ScientificMatch {
            taxon_id: matched.taxon_id,
            existing_type: matched.name_type,
        })
        .collect())
}

fn load_scientific_candidates(
    transaction: &Transaction<'_>,
    matches: Vec<ScientificMatch>,
) -> CoreResult<Vec<ScientificCandidate>> {
    let taxon_ids = matches
        .iter()
        .map(|matched| matched.taxon_id)
        .collect::<Vec<_>>();
    let summaries = load_taxon_summaries(transaction, &taxon_ids)?;
    Ok(matches
        .into_iter()
        .zip(summaries)
        .map(|(matched, summary)| ScientificCandidate {
            summary,
            existing_type: matched.existing_type,
        })
        .collect())
}

fn disambiguate_candidates(
    transaction: &Transaction<'_>,
    mut candidates: Vec<ScientificCandidate>,
    input: &NormalizedInput,
    rank: TaxonRank,
) -> CoreResult<Vec<ScientificCandidate>> {
    for ancestor_rank in TaxonRank::ALL[..rank.index()].iter().rev().copied() {
        let Some(expected) = input.path[ancestor_rank.index()].as_deref() else {
            continue;
        };
        let allowed_ancestor_ids =
            find_preferred_scientific_matches(transaction, ancestor_rank, expected)?
                .into_iter()
                .map(|matched| matched.taxon_id)
                .collect::<HashSet<_>>();
        candidates.retain(|candidate| {
            candidate.summary.breadcrumb.iter().any(|ancestor| {
                ancestor.rank == ancestor_rank && allowed_ancestor_ids.contains(&ancestor.taxon_id)
            })
        });
        if candidates.len() <= 1 {
            break;
        }
    }
    Ok(candidates)
}

fn resolve_or_create_lineage(
    transaction: &Transaction<'_>,
    input: &NormalizedInput,
    rank: TaxonRank,
    child_rank: TaxonRank,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<Result<TaxonSummary, LineageFailure>> {
    let Some(name) = input.path[rank.index()].as_deref() else {
        return Ok(Err(LineageFailure::MissingParent {
            child_rank,
            parent_rank: rank,
        }));
    };
    let matches = find_preferred_scientific_matches(transaction, rank, name)?;
    let mut candidates = if matches.is_empty() {
        Vec::new()
    } else {
        load_scientific_candidates(transaction, matches)?
    };
    if candidates.len() > 1 {
        candidates = disambiguate_candidates(transaction, candidates, input, rank)?;
    }
    match candidates.as_slice() {
        [candidate] => return Ok(Ok(candidate.summary.clone())),
        [_, _, ..] => {
            return Ok(Err(LineageFailure::MultipleCandidates(
                candidates
                    .into_iter()
                    .map(|candidate| candidate.summary)
                    .collect(),
            )));
        }
        [] => {}
    }

    let parent = if let Some(parent_rank) = rank.parent() {
        if input.path[parent_rank.index()].is_none() {
            return Ok(Err(LineageFailure::MissingParent {
                child_rank: rank,
                parent_rank,
            }));
        }
        match resolve_or_create_lineage(transaction, input, parent_rank, rank, changes)? {
            Ok(parent) => Some(parent),
            Err(failure) => return Ok(Err(failure)),
        }
    } else {
        None
    };
    insert_taxon_with_scientific_name(
        transaction,
        rank,
        name,
        parent.as_ref(),
        None,
        None,
        None,
        changes,
    )
    .map(Ok)
}

#[allow(clippy::too_many_arguments)]
fn insert_taxon_with_scientific_name(
    transaction: &Transaction<'_>,
    rank: TaxonRank,
    name: &str,
    parent: Option<&TaxonSummary>,
    geological_range: Option<&str>,
    authority_year: Option<&str>,
    source: Option<&str>,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<TaxonSummary> {
    transaction.execute(
        "INSERT INTO taxa (parent_taxon_id, rank, geological_range) VALUES (?, ?, ?)",
        params![
            parent.map(|value| value.taxon_id),
            rank.code(),
            geological_range
        ],
    )?;
    let taxon_id = transaction.last_insert_rowid();
    transaction.execute(
        r#"
        INSERT INTO taxon_names (taxon_id, name_type, name, authority_year, source)
        VALUES (?, ?, ?, ?, ?)
        "#,
        params![
            taxon_id,
            TaxonomyNameType::SciName.code(),
            name,
            authority_year,
            source
        ],
    )?;
    changes.extend([
        TaxonChange {
            kind: TaxonChangeKind::CreateTaxon,
            field: "taxon".into(),
            old_value: None,
            new_value: Some(name.into()),
        },
        TaxonChange {
            kind: TaxonChangeKind::AppendName,
            field: "sci_name".into(),
            old_value: None,
            new_value: Some(name.into()),
        },
    ]);
    load_taxon_summary(transaction, taxon_id)?
        .ok_or_else(|| CoreError::NotFound(format!("new {} taxon {taxon_id}", rank.as_str())))
}

enum MatchResult {
    None,
    One(TaxonSummary, MatchedName),
    Many(Vec<TaxonSummary>),
}

#[derive(Debug)]
struct ScientificMatch {
    taxon_id: i64,
    existing_type: TaxonomyNameType,
}

#[derive(Debug)]
struct ScientificCandidate {
    summary: TaxonSummary,
    existing_type: TaxonomyNameType,
}

enum LineageFailure {
    MissingParent {
        child_rank: TaxonRank,
        parent_rank: TaxonRank,
    },
    MultipleCandidates(Vec<TaxonSummary>),
}

#[derive(Debug, Clone)]
struct ParsedSynonym {
    name: String,
    authority_year: Option<String>,
}

#[derive(Debug)]
struct MatchedName {
    input_index: usize,
    name: String,
    authority_year: Option<String>,
    existing_type: TaxonomyNameType,
}

#[derive(Debug)]
struct NormalizedInput {
    path: [Option<String>; 5],
    target_rank: TaxonRank,
    target_name: String,
    authority_year: Option<String>,
    synonyms: Vec<ParsedSynonym>,
    zh_names: Vec<String>,
    en_names: Vec<String>,
    geological_range: Option<String>,
    source: Option<String>,
}

impl NormalizedInput {
    fn from_row(
        row: &TaxonInputRow,
        synonym_parser: &SynonymAuthorityParser,
    ) -> Result<Self, String> {
        let mut path = [
            normalize_name(row.kingdom.as_deref()),
            normalize_name(row.order.as_deref()),
            normalize_name(row.family.as_deref()),
            normalize_name(row.genus.as_deref()),
            normalize_name(row.species.as_deref()),
        ];
        let target_index = path
            .iter()
            .rposition(Option::is_some)
            .ok_or_else(|| "at least one scientific rank field is required".to_string())?;
        let target_rank = TaxonRank::ALL[target_index];
        if target_rank == TaxonRank::Species && path[3].is_none() {
            path[3] = path[4]
                .as_deref()
                .and_then(|value| value.split_whitespace().next())
                .map(str::to_string);
        }
        let target_name = path[target_index].clone().unwrap_or_default();
        let mut seen_scientific_names = HashSet::from([target_name.clone()]);
        let mut synonyms = Vec::new();
        for raw in &row.synonyms {
            let parts = synonym_parser
                .split(raw)
                .map_err(|error| error.to_string())?;
            if seen_scientific_names.insert(parts.name.clone()) {
                synonyms.push(ParsedSynonym {
                    name: parts.name,
                    authority_year: normalize_text(parts.authority_year.as_deref()),
                });
            }
        }
        let zh_names = combined_names(row.zh_name.as_deref(), &row.zh_alias);
        let en_names = combined_names(row.en_name.as_deref(), &row.en_alias);
        Ok(Self {
            path,
            target_rank,
            target_name,
            authority_year: normalize_text(row.authority_year.as_deref()),
            synonyms,
            zh_names,
            en_names,
            geological_range: normalize_text(row.geological_range.as_deref()),
            source: normalize_text(row.source.as_deref()),
        })
    }

    fn scientific_names(&self) -> Vec<ParsedSynonym> {
        std::iter::once(ParsedSynonym {
            name: self.target_name.clone(),
            authority_year: self.authority_year.clone(),
        })
        .chain(self.synonyms.iter().cloned())
        .collect()
    }
}

fn combined_names(primary: Option<&str>, aliases: &[String]) -> Vec<String> {
    let mut values = Vec::with_capacity(aliases.len() + 1);
    if let Some(primary) = normalize_name(primary) {
        values.push(primary);
    }
    values.extend(unique_names(aliases));
    deduplicate(values)
}

fn unique_names(values: &[String]) -> Vec<String> {
    deduplicate(
        values
            .iter()
            .filter_map(|value| normalize_name(Some(value)))
            .collect(),
    )
}

fn deduplicate(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn normalize_name(value: Option<&str>) -> Option<String> {
    value.and_then(normalize_taxonomy_name)
}

fn classify_changes(changes: &[TaxonChange]) -> Vec<TaxonRowStatus> {
    if changes.is_empty() {
        return vec![TaxonRowStatus::NoChange];
    }
    let mut types = Vec::new();
    if changes
        .iter()
        .any(|change| change.kind == TaxonChangeKind::Supplement)
    {
        types.push(TaxonRowStatus::Supplement);
    }
    if changes
        .iter()
        .any(|change| change.kind == TaxonChangeKind::AppendName)
    {
        types.push(TaxonRowStatus::NewName);
    }
    if changes
        .iter()
        .any(|change| change.kind == TaxonChangeKind::Overwrite)
    {
        types.push(TaxonRowStatus::Overwrite);
    }
    types
}

fn failed_outcome(
    row_number: usize,
    operation_type: TaxonRowStatus,
    message: impl Into<String>,
) -> TaxonRowOutcome {
    TaxonRowOutcome {
        row_number,
        operation_types: vec![operation_type],
        message: message.into(),
        target: None,
        parent: None,
        candidates: Vec::new(),
        changes: Vec::new(),
    }
}

fn candidate_message(candidates: &[TaxonSummary]) -> String {
    let names = candidates
        .iter()
        .map(|candidate| {
            candidate
                .names
                .sci_name
                .clone()
                .unwrap_or_else(|| candidate.taxon_id.to_string())
        })
        .collect::<Vec<_>>()
        .join("/");
    format!("multiple candidates: {names}")
}

fn describe_changes(changes: &[TaxonChange]) -> String {
    changes
        .iter()
        .map(|change| match (&change.old_value, &change.new_value) {
            (None, Some(new)) => format!("{} added: {}", change.field, new),
            (Some(old), Some(new)) => format!("{}: {} -> {}", change.field, old, new),
            (Some(old), None) => format!("{} removed: {}", change.field, old),
            (None, None) => format!("{} changed", change.field),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn get_taxonomy_name_separator(database: &Database) -> CoreResult<String> {
    Ok(crate::metadata::get_raw(
        &database.connect_taxonomy_metadata_context()?,
        crate::metadata::MetadataKey::TaxonomyNameSeparator,
    )?
    .unwrap_or_else(|| ";".to_string()))
}

pub fn set_taxonomy_name_separator(database: &Database, separator: &str) -> CoreResult<()> {
    let mut characters = separator.chars();
    let Some(character) = characters.next() else {
        return Err(CoreError::InvalidArgument(
            "name separator is required".into(),
        ));
    };
    if characters.next().is_some() || character == '|' || character.is_whitespace() {
        return Err(CoreError::InvalidArgument(
            "name separator must be one non-whitespace character other than '|'".into(),
        ));
    }
    crate::metadata::set_raw(
        &database.connect_taxonomy_metadata_context()?,
        crate::metadata::MetadataKey::TaxonomyNameSeparator,
        separator,
    )
}

pub fn taxonomy_formatted_update_template(database: &Database) -> CoreResult<String> {
    let mut writer = WriterBuilder::new()
        .delimiter(crate::general::get_csv_delimiter_byte(database)?)
        .from_writer(Vec::new());
    writer.write_record(TAXONOMY_INPUT_COLUMNS)?;
    writer.flush()?;
    String::from_utf8(writer.into_inner().map_err(|error| error.into_error())?)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid UTF-8 template: {error}")))
}

pub fn parse_taxonomy_input_csv(
    database: &Database,
    input: &str,
) -> CoreResult<Vec<TaxonInputRow>> {
    let separator = get_taxonomy_name_separator(database)?;
    let separator = separator.chars().next().unwrap_or(';');
    let mut reader = ReaderBuilder::new()
        .delimiter(crate::general::get_csv_delimiter_byte(database)?)
        .from_reader(input.as_bytes());
    let headers = reader.headers()?.clone();
    if headers.is_empty() {
        return Err(CoreError::InvalidArgument("CSV header is required".into()));
    }
    let allowed = TAXONOMY_INPUT_COLUMNS.into_iter().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    for header in &headers {
        if !allowed.contains(header) {
            return Err(CoreError::InvalidArgument(format!(
                "unknown taxonomy input column: {header}"
            )));
        }
        if !seen.insert(header.to_string()) {
            return Err(CoreError::InvalidArgument(format!(
                "duplicate taxonomy input column: {header}"
            )));
        }
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let mut row = TaxonInputRow::default();
        for (header, value) in headers.iter().zip(record.iter()) {
            let scalar = || normalize_text(Some(value));
            let normalized_multiple = || {
                value
                    .split(separator)
                    .filter_map(|value| normalize_name(Some(value)))
                    .collect::<Vec<_>>()
            };
            let raw_multiple = || {
                if value.is_empty() {
                    Vec::new()
                } else {
                    value
                        .split(separator)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                }
            };
            match header {
                "kingdom" => row.kingdom = scalar(),
                "order" => row.order = scalar(),
                "family" => row.family = scalar(),
                "genus" => row.genus = scalar(),
                "species" => row.species = scalar(),
                "authority_year" => row.authority_year = scalar(),
                "synonyms" => row.synonyms = raw_multiple(),
                "zh_name" => row.zh_name = scalar(),
                "zh_alias" => row.zh_alias = normalized_multiple(),
                "en_name" => row.en_name = scalar(),
                "en_alias" => row.en_alias = normalized_multiple(),
                "geological_range" => row.geological_range = scalar(),
                "source" => row.source = scalar(),
                _ => {}
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

pub fn taxonomy_log_csv(database: &Database, rows: &[TaxonRowOutcome]) -> CoreResult<String> {
    let mut writer = WriterBuilder::new()
        .delimiter(crate::general::get_csv_delimiter_byte(database)?)
        .from_writer(Vec::new());
    writer.write_record([
        "row_number",
        "operation_types",
        "summary",
        "parent",
        "changes",
        "message",
    ])?;
    for row in rows {
        writer.write_record([
            row.row_number.to_string(),
            serialize_json(&row.operation_types, "taxonomy row operation types")?,
            serialize_json(&row.target, "taxonomy row target")?,
            serialize_json(&row.parent, "taxonomy row parent")?,
            serialize_json(&row.changes, "taxonomy row changes")?,
            row.message.clone(),
        ])?;
    }
    writer.flush()?;
    String::from_utf8(writer.into_inner().map_err(|error| error.into_error())?)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid UTF-8 log: {error}")))
}

pub fn list_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<OperationPage<OperationSummary>> {
    operations::list_operations(
        &database.connect_taxonomy_metadata_context()?,
        cursor,
        limit,
    )
}

pub fn list_operation_audit(
    database: &Database,
    operation_id: i64,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<OperationPage<OperationAuditRow>> {
    operations::list_operation_audit(
        &database.connect_taxonomy_metadata_context()?,
        operation_id,
        cursor,
        limit,
    )
}

pub fn rollback_operation(database: &Database, operation_id: i64) -> CoreResult<()> {
    let _guard = database.try_taxonomy_mutation()?;
    let mut connection = database.connect_taxonomy_metadata_context()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let summary = operations::get_operation(&transaction, operation_id)?
        .ok_or_else(|| CoreError::NotFound(format!("operation {operation_id}")))?;
    if !summary.rollbackable {
        return Err(CoreError::InvalidArgument(format!(
            "operation {operation_id} cannot be rolled back"
        )));
    }
    let changeset_blob = transaction
        .query_row(
            "SELECT changeset_blob FROM operation_changesets WHERE operation_id = ?",
            [operation_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Consistency(format!(
                "operation {operation_id} has no rollback changeset"
            ))
        })?;
    let affected_taxon_ids = affected_taxon_ids_from_changeset(&transaction, &changeset_blob)?;
    if !changeset_blob.is_empty() {
        let mut inverted = Vec::new();
        invert_strm(&mut Cursor::new(&changeset_blob), &mut inverted)?;
        transaction.apply_strm(
            &mut Cursor::new(inverted),
            Some(is_taxonomy_session_table),
            |conflict_type, item| match item.op() {
                Ok(operation)
                    if conflict_type == ConflictType::SQLITE_CHANGESET_NOTFOUND
                        && operation.code() == Action::SQLITE_DELETE =>
                {
                    ConflictAction::SQLITE_CHANGESET_OMIT
                }
                _ => ConflictAction::SQLITE_CHANGESET_ABORT,
            },
        )?;
    }
    validate_taxonomy(&transaction)?;
    super::sync::record_event(&transaction, None, affected_taxon_ids, false)?;
    operations::delete_operation(&transaction, operation_id)?;
    transaction.commit()?;
    Ok(())
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaxonomyValidationIssue {
    pub code: &'static str,
    pub message: String,
    pub taxon_id: Option<i64>,
    pub related_taxon_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TaxonomyValidationOptions {
    pub check_parent_structure: bool,
    pub check_scientific_name_count: bool,
    pub check_localized_accepted_name_count: bool,
    pub check_localized_alias_dependencies: bool,
    pub check_duplicate_name_family: bool,
    pub check_orphan_names: bool,
    pub require_normalized_names: bool,
}

impl TaxonomyValidationOptions {
    pub const fn full() -> Self {
        Self {
            check_parent_structure: true,
            check_scientific_name_count: true,
            check_localized_accepted_name_count: true,
            check_localized_alias_dependencies: true,
            check_duplicate_name_family: true,
            check_orphan_names: true,
            require_normalized_names: true,
        }
    }

    pub const fn sql_import_staging() -> Self {
        Self {
            check_parent_structure: true,
            check_scientific_name_count: true,
            check_localized_accepted_name_count: true,
            check_localized_alias_dependencies: true,
            check_duplicate_name_family: false,
            check_orphan_names: true,
            require_normalized_names: false,
        }
    }
}

pub(super) fn visit_taxonomy_validation_issues(
    connection: &Connection,
    require_normalized_names: bool,
    visit: impl FnMut(TaxonomyValidationIssue) -> bool,
) -> CoreResult<()> {
    let mut options = TaxonomyValidationOptions::full();
    options.require_normalized_names = require_normalized_names;
    visit_taxonomy_validation_issues_with_progress(connection, options, |_| {}, visit)
}

#[cfg(test)]
pub(super) fn visit_taxonomy_validation_issues_with_options(
    connection: &Connection,
    options: TaxonomyValidationOptions,
    visit: impl FnMut(TaxonomyValidationIssue) -> bool,
) -> CoreResult<()> {
    visit_taxonomy_validation_issues_with_progress(connection, options, |_| {}, visit)
}

pub(super) fn visit_taxonomy_validation_issues_with_progress(
    connection: &Connection,
    options: TaxonomyValidationOptions,
    progress: impl FnMut(OperationProgress),
    visit: impl FnMut(TaxonomyValidationIssue) -> bool,
) -> CoreResult<()> {
    visit_taxonomy_validation_issues_with_progress_internal(
        connection, options, progress, None, visit,
    )
}

pub(super) fn visit_taxonomy_validation_issues_with_progress_and_cancellation(
    connection: &Connection,
    options: TaxonomyValidationOptions,
    progress: impl FnMut(OperationProgress),
    cancellation: &CancellationToken,
    visit: impl FnMut(TaxonomyValidationIssue) -> bool,
) -> CoreResult<()> {
    visit_taxonomy_validation_issues_with_progress_internal(
        connection,
        options,
        progress,
        Some(cancellation),
        visit,
    )
}

fn visit_taxonomy_validation_issues_with_progress_internal(
    connection: &Connection,
    options: TaxonomyValidationOptions,
    mut progress: impl FnMut(OperationProgress),
    cancellation: Option<&CancellationToken>,
    mut visit: impl FnMut(TaxonomyValidationIssue) -> bool,
) -> CoreResult<()> {
    if let Some(cancellation) = cancellation {
        cancellation.check()?;
    }
    progress(validation_progress(
        "loading_taxonomy_structure",
        None,
        None,
        None,
    ));
    let taxa = connection
        .prepare("SELECT taxon_id, parent_taxon_id, rank FROM taxa ORDER BY taxon_id")?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let by_id = taxa
        .iter()
        .map(|(taxon_id, parent_taxon_id, rank)| (*taxon_id, (*parent_taxon_id, *rank)))
        .collect::<HashMap<_, _>>();
    let total_taxa = taxa.len() as u64;
    progress(validation_progress(
        "loading_taxonomy_structure",
        Some(total_taxa),
        Some(total_taxa),
        Some(OperationProgressUnit::Taxa),
    ));
    if options.check_parent_structure {
        progress(validation_progress(
            "checking_parent_cycles",
            Some(0),
            Some(total_taxa),
            Some(OperationProgressUnit::Taxa),
        ));
        let cycle_taxa = cycle_taxon_ids_with_progress_and_cancellation(
            &by_id,
            |current, total| {
                progress(validation_progress(
                    "checking_parent_cycles",
                    Some(current),
                    Some(total),
                    Some(OperationProgressUnit::Taxa),
                ));
            },
            cancellation,
        )?;
        progress(validation_progress(
            "checking_parent_relationships",
            Some(0),
            Some(total_taxa),
            Some(OperationProgressUnit::Taxa),
        ));
        for (index, (taxon_id, parent_taxon_id, rank)) in taxa.iter().enumerate() {
            if (index + 1).is_multiple_of(10_000) {
                if let Some(cancellation) = cancellation {
                    cancellation.check()?;
                }
                progress(validation_progress(
                    "checking_parent_relationships",
                    Some((index + 1) as u64),
                    Some(total_taxa),
                    Some(OperationProgressUnit::Taxa),
                ));
            }
            if cycle_taxa.contains(&taxon_id) {
                if !visit(TaxonomyValidationIssue {
                    code: "parent_cycle",
                    message: format!("Taxon {taxon_id} belongs to a cyclic parent relationship."),
                    taxon_id: Some(*taxon_id),
                    related_taxon_id: *parent_taxon_id,
                }) {
                    return Ok(());
                }
                continue;
            }
            if *rank == TaxonRank::Kingdom.code() {
                if parent_taxon_id.is_some()
                    && !visit(TaxonomyValidationIssue {
                        code: "kingdom_has_parent",
                        message: format!("Kingdom taxon {taxon_id} must be a root taxon."),
                        taxon_id: Some(*taxon_id),
                        related_taxon_id: *parent_taxon_id,
                    })
                {
                    return Ok(());
                }
                continue;
            }
            let Some(parent_taxon_id) = parent_taxon_id else {
                if !visit(TaxonomyValidationIssue {
                    code: "missing_parent",
                    message: format!("Taxon {taxon_id} must have a parent taxon."),
                    taxon_id: Some(*taxon_id),
                    related_taxon_id: None,
                }) {
                    return Ok(());
                }
                continue;
            };
            let Some((_, parent_rank)) = by_id.get(parent_taxon_id) else {
                if !visit(TaxonomyValidationIssue {
                    code: "parent_not_found",
                    message: format!(
                        "Taxon {taxon_id} references missing parent taxon {parent_taxon_id}."
                    ),
                    taxon_id: Some(*taxon_id),
                    related_taxon_id: Some(*parent_taxon_id),
                }) {
                    return Ok(());
                }
                continue;
            };
            if *parent_rank >= *rank
                && !visit(TaxonomyValidationIssue {
                    code: "invalid_parent_rank",
                    message: format!("Taxon {taxon_id} must have a parent with a higher rank."),
                    taxon_id: Some(*taxon_id),
                    related_taxon_id: Some(*parent_taxon_id),
                })
            {
                return Ok(());
            }
        }
        progress(validation_progress(
            "checking_parent_relationships",
            Some(total_taxa),
            Some(total_taxa),
            Some(OperationProgressUnit::Taxa),
        ));
    }
    if options.check_scientific_name_count {
        progress(validation_progress(
            "checking_scientific_names",
            None,
            None,
            None,
        ));
        let mut invalid_sci_names = connection.prepare(
            r#"
            SELECT taxa.taxon_id
            FROM taxa
            LEFT JOIN taxon_names
              ON taxon_names.taxon_id = taxa.taxon_id
             AND taxon_names.name_type = 1
            GROUP BY taxa.taxon_id
            HAVING COUNT(taxon_names.name_id) != 1
            ORDER BY taxa.taxon_id
            "#,
        )?;
        for row in invalid_sci_names.query_map([], |row| row.get::<_, i64>(0))? {
            let taxon_id = row?;
            if !visit(TaxonomyValidationIssue {
                code: "invalid_sci_name_count",
                message: format!("Taxon {taxon_id} must have exactly one scientific name."),
                taxon_id: Some(taxon_id),
                related_taxon_id: None,
            }) {
                return Ok(());
            }
        }
    }
    if options.check_localized_accepted_name_count || options.check_localized_alias_dependencies {
        progress(validation_progress(
            "checking_localized_names",
            None,
            None,
            None,
        ));
    }
    if options.check_localized_accepted_name_count {
        let mut duplicate_accepted_names = connection.prepare(
            r#"
            SELECT taxon_id, name_type
            FROM taxon_names
            WHERE name_type IN (?, ?)
            GROUP BY taxon_id, name_type
            HAVING COUNT(name_id) > 1
            ORDER BY taxon_id, name_type
            "#,
        )?;
        for row in duplicate_accepted_names.query_map(
            params![
                TaxonomyNameType::ZhName.code(),
                TaxonomyNameType::EnName.code()
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )? {
            let (taxon_id, name_type) = row?;
            let (code, language) = if name_type == TaxonomyNameType::ZhName.code() {
                ("invalid_zh_name_count", "Chinese")
            } else {
                ("invalid_en_name_count", "English")
            };
            if !visit(TaxonomyValidationIssue {
                code,
                message: format!(
                    "Taxon {taxon_id} must have at most one {language} accepted name."
                ),
                taxon_id: Some(taxon_id),
                related_taxon_id: None,
            }) {
                return Ok(());
            }
        }
    }
    if options.check_localized_alias_dependencies {
        let localized_name_types = [
            (
                TaxonomyNameType::ZhAlias,
                TaxonomyNameType::ZhName,
                "zh_alias_without_accepted_name",
                "Chinese",
            ),
            (
                TaxonomyNameType::EnAlias,
                TaxonomyNameType::EnName,
                "en_alias_without_accepted_name",
                "English",
            ),
        ];
        let mut aliases_without_accepted_name = connection.prepare(
            r#"
            SELECT DISTINCT alias.taxon_id
            FROM taxon_names AS alias
            WHERE alias.name_type = ?
              AND NOT EXISTS (
                SELECT 1
                FROM taxon_names AS accepted
                WHERE accepted.taxon_id = alias.taxon_id
                  AND accepted.name_type = ?
              )
            ORDER BY alias.taxon_id
            "#,
        )?;
        for (alias_type, accepted_type, code, language) in localized_name_types {
            for row in aliases_without_accepted_name
                .query_map(params![alias_type.code(), accepted_type.code()], |row| {
                    row.get::<_, i64>(0)
                })?
            {
                let taxon_id = row?;
                if !visit(TaxonomyValidationIssue {
                    code,
                    message: format!(
                        "Taxon {taxon_id} has {language} aliases but no {language} accepted name."
                    ),
                    taxon_id: Some(taxon_id),
                    related_taxon_id: None,
                }) {
                    return Ok(());
                }
            }
        }
    }
    if options.check_duplicate_name_family {
        progress(validation_progress(
            "checking_duplicate_names",
            None,
            None,
            None,
        ));
        let mut duplicate_family_names = connection.prepare(
            r#"
            SELECT taxon_id, (name_type + 1) / 2 AS name_family, name
            FROM taxon_names
            GROUP BY taxon_id, name_family, name
            HAVING COUNT(name_id) > 1
            ORDER BY taxon_id, name_family, name
            "#,
        )?;
        for row in duplicate_family_names.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (taxon_id, name_family, name) = row?;
            let family = match name_family {
                1 => "scientific",
                2 => "Chinese",
                3 => "English",
                _ => "invalid",
            };
            if !visit(TaxonomyValidationIssue {
                code: "duplicate_name_family",
                message: format!("Taxon {taxon_id} contains duplicate {family} name '{name}'."),
                taxon_id: Some(taxon_id),
                related_taxon_id: None,
            }) {
                return Ok(());
            }
        }
    }
    if options.check_orphan_names {
        progress(validation_progress(
            "checking_orphan_names",
            None,
            None,
            None,
        ));
        let mut orphan_name_taxa = connection.prepare(
            r#"
            SELECT DISTINCT taxon_names.taxon_id
            FROM taxon_names
            LEFT JOIN taxa ON taxa.taxon_id = taxon_names.taxon_id
            WHERE taxa.taxon_id IS NULL
            ORDER BY taxon_names.taxon_id
            "#,
        )?;
        for row in orphan_name_taxa.query_map([], |row| row.get::<_, i64>(0))? {
            let taxon_id = row?;
            if !visit(TaxonomyValidationIssue {
                code: "name_taxon_not_found",
                message: format!("Taxon names reference missing taxon {taxon_id}."),
                taxon_id: Some(taxon_id),
                related_taxon_id: None,
            }) {
                return Ok(());
            }
        }
    }
    if options.require_normalized_names {
        progress(validation_progress(
            "checking_normalized_names",
            None,
            None,
            None,
        ));
        let mut statement =
            connection.prepare("SELECT name_id, taxon_id, name FROM taxon_names")?;
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (name_id, taxon_id, name) = row?;
            if normalize_name(Some(&name)).as_deref() != Some(name.as_str())
                && !visit(TaxonomyValidationIssue {
                    code: "name_not_normalized",
                    message: format!("Taxon name {name_id} is not normalized."),
                    taxon_id: Some(taxon_id),
                    related_taxon_id: None,
                })
            {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn validation_progress(
    stage: &str,
    current: Option<u64>,
    total: Option<u64>,
    unit: Option<OperationProgressUnit>,
) -> OperationProgress {
    OperationProgress {
        stage: stage.into(),
        current,
        total,
        unit,
    }
}

#[cfg(test)]
fn cycle_taxon_ids(by_id: &HashMap<i64, (Option<i64>, i64)>) -> HashSet<i64> {
    cycle_taxon_ids_with_progress(by_id, |_, _| {})
}

#[cfg(test)]
fn cycle_taxon_ids_with_progress(
    by_id: &HashMap<i64, (Option<i64>, i64)>,
    progress: impl FnMut(u64, u64),
) -> HashSet<i64> {
    cycle_taxon_ids_with_progress_and_cancellation(by_id, progress, None)
        .expect("cycle detection without cancellation cannot fail")
}

fn cycle_taxon_ids_with_progress_and_cancellation(
    by_id: &HashMap<i64, (Option<i64>, i64)>,
    mut progress: impl FnMut(u64, u64),
    cancellation: Option<&CancellationToken>,
) -> CoreResult<HashSet<i64>> {
    let mut states = HashMap::<i64, VisitState>::new();
    let mut cycle_taxa = HashSet::new();
    let total = by_id.len() as u64;
    let mut completed = 0_u64;
    for &origin_taxon_id in by_id.keys() {
        if states.get(&origin_taxon_id) == Some(&VisitState::Done) {
            continue;
        }
        let mut path = Vec::new();
        let mut path_positions = HashMap::new();
        let mut current_taxon_id = Some(origin_taxon_id);
        while let Some(taxon_id) = current_taxon_id {
            if let Some(&position) = path_positions.get(&taxon_id) {
                cycle_taxa.extend(path[position..].iter().copied());
                break;
            }
            if states.get(&taxon_id) == Some(&VisitState::Done) || !by_id.contains_key(&taxon_id) {
                break;
            }
            states.insert(taxon_id, VisitState::Visiting);
            path_positions.insert(taxon_id, path.len());
            path.push(taxon_id);
            current_taxon_id = by_id[&taxon_id].0;
        }
        for taxon_id in path {
            states.insert(taxon_id, VisitState::Done);
            completed += 1;
            if completed.is_multiple_of(10_000) {
                if let Some(cancellation) = cancellation {
                    cancellation.check()?;
                }
                progress(completed, total);
            }
        }
    }
    progress(total, total);
    Ok(cycle_taxa)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

pub(super) fn validate_taxonomy(connection: &Connection) -> CoreResult<()> {
    let mut first_issue = None;
    visit_taxonomy_validation_issues(connection, true, |issue| {
        first_issue = Some(issue);
        false
    })?;
    if let Some(issue) = first_issue {
        return Err(CoreError::InvalidArgument(issue.message));
    }
    Ok(())
}

pub(super) fn validate_taxonomy_changes_with_cancellation(
    connection: &Connection,
    affected_taxon_ids: &BTreeSet<i64>,
    cancellation: &CancellationToken,
) -> CoreResult<()> {
    cancellation.check()?;
    connection.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS vividarium_validation_taxa(taxon_id INTEGER PRIMARY KEY); DELETE FROM vividarium_validation_taxa;",
    )?;
    {
        let mut insert = connection
            .prepare("INSERT OR IGNORE INTO vividarium_validation_taxa(taxon_id) VALUES (?)")?;
        for (index, taxon_id) in affected_taxon_ids.iter().enumerate() {
            if index.is_multiple_of(1_000) {
                cancellation.check()?;
            }
            insert.execute([taxon_id])?;
        }
    }

    let taxa = connection
        .prepare(
            r#"
            WITH RECURSIVE relevant(taxon_id) AS (
                SELECT taxon_id FROM vividarium_validation_taxa
                UNION
                SELECT taxa.taxon_id
                FROM taxa
                JOIN vividarium_validation_taxa AS changed
                  ON changed.taxon_id = taxa.parent_taxon_id
                UNION
                SELECT taxa.parent_taxon_id
                FROM taxa JOIN relevant ON taxa.taxon_id = relevant.taxon_id
                WHERE taxa.parent_taxon_id IS NOT NULL
            )
            SELECT taxa.taxon_id, taxa.parent_taxon_id, taxa.rank
            FROM taxa JOIN relevant USING (taxon_id)
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let by_id = taxa
        .iter()
        .map(|(taxon_id, parent_taxon_id, rank)| (*taxon_id, (*parent_taxon_id, *rank)))
        .collect::<HashMap<_, _>>();
    let cycles =
        cycle_taxon_ids_with_progress_and_cancellation(&by_id, |_, _| {}, Some(cancellation))?;
    if let Some(taxon_id) = cycles.iter().next() {
        return Err(CoreError::InvalidArgument(format!(
            "Taxon {taxon_id} belongs to a cyclic parent relationship."
        )));
    }
    for (index, (taxon_id, parent_taxon_id, rank)) in taxa.iter().enumerate() {
        if index.is_multiple_of(1_000) {
            cancellation.check()?;
        }
        if *rank == TaxonRank::Kingdom.code() {
            if parent_taxon_id.is_some() {
                return Err(CoreError::InvalidArgument(format!(
                    "Kingdom taxon {taxon_id} must be a root taxon."
                )));
            }
            continue;
        }
        let Some(parent_taxon_id) = parent_taxon_id else {
            return Err(CoreError::InvalidArgument(format!(
                "Taxon {taxon_id} must have a parent taxon."
            )));
        };
        let Some((_, parent_rank)) = by_id.get(parent_taxon_id) else {
            return Err(CoreError::InvalidArgument(format!(
                "Taxon {taxon_id} references missing parent taxon {parent_taxon_id}."
            )));
        };
        if *parent_rank >= *rank {
            return Err(CoreError::InvalidArgument(format!(
                "Taxon {taxon_id} must have a parent with a higher rank."
            )));
        }
    }

    let invalid_name = connection
        .query_row(
            r#"
            SELECT issue FROM (
                SELECT 'Taxon ' || taxa.taxon_id || ' must have exactly one scientific name.' AS issue
                FROM taxa
                JOIN vividarium_validation_taxa AS changed USING (taxon_id)
                LEFT JOIN taxon_names ON taxon_names.taxon_id = taxa.taxon_id AND taxon_names.name_type = 1
                GROUP BY taxa.taxon_id HAVING COUNT(taxon_names.name_id) != 1
                UNION ALL
                SELECT 'Taxon ' || names.taxon_id || ' must have at most one localized accepted name.'
                FROM taxon_names AS names
                JOIN vividarium_validation_taxa AS changed USING (taxon_id)
                WHERE names.name_type IN (3, 5)
                GROUP BY names.taxon_id, names.name_type HAVING COUNT(names.name_id) > 1
                UNION ALL
                SELECT 'Taxon ' || alias.taxon_id || ' has aliases but no accepted localized name.'
                FROM taxon_names AS alias
                JOIN vividarium_validation_taxa AS changed USING (taxon_id)
                WHERE (alias.name_type = 4 AND NOT EXISTS (
                    SELECT 1 FROM taxon_names accepted WHERE accepted.taxon_id = alias.taxon_id AND accepted.name_type = 3
                )) OR (alias.name_type = 6 AND NOT EXISTS (
                    SELECT 1 FROM taxon_names accepted WHERE accepted.taxon_id = alias.taxon_id AND accepted.name_type = 5
                ))
                UNION ALL
                SELECT 'Taxon ' || names.taxon_id || ' contains duplicate names in one name family.'
                FROM taxon_names AS names
                JOIN vividarium_validation_taxa AS changed USING (taxon_id)
                GROUP BY names.taxon_id, (names.name_type + 1) / 2, names.name
                HAVING COUNT(names.name_id) > 1
                UNION ALL
                SELECT 'Taxon names reference missing taxon ' || names.taxon_id || '.'
                FROM taxon_names AS names
                JOIN vividarium_validation_taxa AS changed USING (taxon_id)
                LEFT JOIN taxa USING (taxon_id)
                WHERE taxa.taxon_id IS NULL
            ) LIMIT 1
            "#,
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(message) = invalid_name {
        return Err(CoreError::InvalidArgument(message));
    }
    let mut names = connection.prepare(
        r#"
        SELECT names.name_id, names.name
        FROM taxon_names AS names
        JOIN vividarium_validation_taxa AS changed USING (taxon_id)
        "#,
    )?;
    for (index, row) in names
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .enumerate()
    {
        if index.is_multiple_of(1_000) {
            cancellation.check()?;
        }
        let (name_id, name) = row?;
        if normalize_name(Some(&name)).as_deref() != Some(name.as_str()) {
            return Err(CoreError::InvalidArgument(format!(
                "Taxon name {name_id} is not normalized."
            )));
        }
    }
    connection.execute_batch("DELETE FROM vividarium_validation_taxa")?;
    Ok(())
}

pub(super) fn validate_taxonomy_with_progress_and_cancellation(
    connection: &Connection,
    progress: impl FnMut(OperationProgress),
    cancellation: &CancellationToken,
) -> CoreResult<()> {
    validate_taxonomy_with_progress_internal(connection, progress, Some(cancellation))
}

fn validate_taxonomy_with_progress_internal(
    connection: &Connection,
    progress: impl FnMut(OperationProgress),
    cancellation: Option<&CancellationToken>,
) -> CoreResult<()> {
    let mut first_issue = None;
    visit_taxonomy_validation_issues_with_progress_internal(
        connection,
        TaxonomyValidationOptions::full(),
        progress,
        cancellation,
        |issue| {
            first_issue = Some(issue);
            false
        },
    )?;
    if let Some(issue) = first_issue {
        return Err(CoreError::InvalidArgument(issue.message));
    }
    Ok(())
}

pub(super) fn serialize_json<T: Serialize + ?Sized>(value: &T, label: &str) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid {label}: {error}")))
}

#[cfg(test)]
#[path = "formatted/tests.rs"]
mod tests;
