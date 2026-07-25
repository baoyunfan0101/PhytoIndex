use std::collections::{BTreeSet, HashSet};
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

use super::name_parser::split_scientific_name_authority;
use super::page::{
    TaxonomyCursor, TaxonomyPage, decode_cursor, encode_cursor, invalid_cursor, page_limit,
};
use super::view::{TaxonSummary, load_taxon_summaries, load_taxon_summary};
use crate::mapping;
use crate::models::OperationInputTable;
use crate::{CoreError, CoreResult, Database};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_taxon_id: Option<i64>,
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

pub type TaxonomyOperationRowLog = TaxonRowOutcome;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyPreviewResult {
    pub delimiter: String,
    pub encoding: String,
    pub rows: Vec<TaxonRowOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyCustomSqlTempTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyOperationSource {
    FormattedUpdate,
}

impl TaxonomyOperationSource {
    fn as_str(self) -> &'static str {
        "formatted_update"
    }

    fn from_str(value: &str) -> CoreResult<Self> {
        match value {
            "formatted_update" => Ok(Self::FormattedUpdate),
            _ => Err(CoreError::InvalidArgument(format!(
                "invalid taxonomy operation source: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyOperationResult {
    pub operation_id: i64,
    pub total_rows: usize,
    pub succeeded_rows: usize,
    pub failed_rows: usize,
    pub delimiter: String,
    pub encoding: String,
    pub rows: Vec<TaxonomyOperationRowLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxonomyOperation {
    pub operation_id: i64,
    pub source: TaxonomyOperationSource,
    pub input: Vec<TaxonInputRow>,
    pub result: TaxonomyOperationResult,
    pub changeset_size: usize,
    pub applied_at: String,
}

pub fn preview_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyPreviewResult> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let outcomes = process_rows(&transaction, rows)?;
    transaction.rollback()?;
    Ok(TaxonomyPreviewResult {
        delimiter: "|".into(),
        encoding: "UTF-8".into(),
        rows: outcomes,
    })
}

pub fn apply_rows(
    database: &Database,
    rows: &[TaxonInputRow],
) -> CoreResult<TaxonomyOperationResult> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut session = start_taxonomy_session(&transaction)?;
    let outcomes = process_rows(&transaction, rows)?;
    validate_taxonomy(&transaction)?;
    let mut changeset_blob = Vec::new();
    session.changeset_strm(&mut changeset_blob)?;
    drop(session);

    let stored_input = rows
        .iter()
        .cloned()
        .map(|mut row| {
            row.selected_taxon_id = None;
            row
        })
        .collect::<Vec<_>>();
    let input_json = serialize_json(&stored_input, "taxonomy operation input")?;
    transaction.execute(
        r#"
        INSERT INTO taxonomy_operations (source, input_json, result_json, changeset_blob)
        VALUES (?, ?, '{}', ?)
        "#,
        params![
            TaxonomyOperationSource::FormattedUpdate.as_str(),
            input_json,
            changeset_blob
        ],
    )?;
    let operation_id = transaction.last_insert_rowid();
    let failed_rows = outcomes
        .iter()
        .filter(|row| row.operation_types.iter().any(|value| value.is_failure()))
        .count();
    let result = TaxonomyOperationResult {
        operation_id,
        total_rows: outcomes.len(),
        succeeded_rows: outcomes.len() - failed_rows,
        failed_rows,
        delimiter: "|".into(),
        encoding: "UTF-8".into(),
        rows: outcomes,
    };
    transaction.execute(
        "UPDATE taxonomy_operations SET result_json = ? WHERE operation_id = ?",
        params![
            serialize_json(&result, "taxonomy operation result")?,
            operation_id
        ],
    )?;
    let affected_taxon_ids = affected_taxon_ids_from_changeset(&transaction, &changeset_blob)?;
    transaction.commit()?;
    mapping::refresh_after_taxonomy_changes(database, affected_taxon_ids)?;
    Ok(result)
}

fn process_rows(
    transaction: &Transaction<'_>,
    rows: &[TaxonInputRow],
) -> CoreResult<Vec<TaxonRowOutcome>> {
    let mut outcomes = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        outcomes.push(process_row(transaction, index + 1, row)?);
    }
    Ok(outcomes)
}

fn process_row(
    transaction: &Transaction<'_>,
    row_number: usize,
    row: &TaxonInputRow,
) -> CoreResult<TaxonRowOutcome> {
    let normalized = match NormalizedInput::from_row(row) {
        Ok(value) => value,
        Err(message) => return Ok(failed_outcome(row_number, TaxonRowStatus::Invalid, message)),
    };
    let target_rank = normalized.target_rank;
    let match_result = if let Some(taxon_id) = row.selected_taxon_id {
        match load_taxon_summary(transaction, taxon_id)? {
            Some(summary) if summary.rank == target_rank => MatchResult::One(summary),
            Some(_) => {
                return Ok(failed_outcome(
                    row_number,
                    TaxonRowStatus::Invalid,
                    "selected taxon rank does not match the input target rank",
                ));
            }
            None => {
                return Ok(failed_outcome(
                    row_number,
                    TaxonRowStatus::NotMatched,
                    format!("selected taxon {taxon_id} was not found"),
                ));
            }
        }
    } else {
        find_target(transaction, &normalized)?
    };

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
        MatchResult::One(summary) => update_existing(transaction, row_number, &normalized, summary),
    }
}

fn create_taxon(
    transaction: &Transaction<'_>,
    row_number: usize,
    input: &NormalizedInput,
) -> CoreResult<TaxonRowOutcome> {
    let parent = match input.target_rank.parent() {
        None => None,
        Some(parent_rank) => {
            let parent_name = input.parent_name().ok_or_else(|| {
                CoreError::InvalidArgument(format!(
                    "new {} taxon requires a {} scientific name",
                    input.target_rank.as_str(),
                    parent_rank.as_str()
                ))
            });
            let parent_name = match parent_name {
                Ok(value) => value,
                Err(error) => {
                    return Ok(failed_outcome(
                        row_number,
                        TaxonRowStatus::NotMatched,
                        error.to_string(),
                    ));
                }
            };
            let candidates = find_scientific_candidates(
                transaction,
                parent_rank,
                parent_name,
                input,
                parent_rank,
            )?;
            match candidates.as_slice() {
                [] => {
                    return Ok(failed_outcome(
                        row_number,
                        TaxonRowStatus::NotMatched,
                        format!(
                            "parent {} '{}' was not found",
                            parent_rank.as_str(),
                            parent_name
                        ),
                    ));
                }
                [parent] => Some(parent.clone()),
                _ => {
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
        }
    };

    transaction.execute(
        "INSERT INTO taxa (parent_taxon_id, rank, geological_range) VALUES (?, ?, ?)",
        params![
            parent.as_ref().map(|value| value.taxon_id),
            input.target_rank.code(),
            input.geological_range
        ],
    )?;
    let taxon_id = transaction.last_insert_rowid();
    transaction.execute(
        r#"
        INSERT INTO taxon_names (taxon_id, name_type, name, authority_year, source)
        VALUES (?, 'sci_name', ?, ?, ?)
        "#,
        params![
            taxon_id,
            input.target_name,
            input.authority_year,
            input.source
        ],
    )?;
    let mut changes = vec![
        TaxonChange {
            kind: TaxonChangeKind::CreateTaxon,
            field: "taxon".into(),
            old_value: None,
            new_value: Some(input.target_name.clone()),
        },
        TaxonChange {
            kind: TaxonChangeKind::AppendName,
            field: "sci_name".into(),
            old_value: None,
            new_value: Some(input.target_name.clone()),
        },
    ];
    apply_additional_names(transaction, taxon_id, input, &mut changes)?;
    let target = load_taxon_summary(transaction, taxon_id)?;
    if let Some(target) = target.as_ref() {
        supplement_path_sources(transaction, input, target, &mut changes)?;
    }
    let mut operation_types = classify_changes(&changes);
    operation_types.insert(0, TaxonRowStatus::NewTaxon);
    Ok(TaxonRowOutcome {
        row_number,
        operation_types,
        message: describe_changes(&changes),
        target,
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
    if let Some(target_name_type) =
        existing_scientific_name_type(transaction, taxon_id, &input.target_name)?
    {
        update_name_fields(
            transaction,
            taxon_id,
            target_name_type,
            &input.target_name,
            input.authority_year.as_deref(),
            input.source.as_deref(),
            &mut changes,
        )?;
    }
    apply_additional_names(transaction, taxon_id, input, &mut changes)?;
    supplement_path_sources(transaction, input, &summary, &mut changes)?;
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

fn supplement_path_sources(
    transaction: &Transaction<'_>,
    input: &NormalizedInput,
    target: &TaxonSummary,
    changes: &mut Vec<TaxonChange>,
) -> CoreResult<()> {
    let Some(source) = input.source.as_deref() else {
        return Ok(());
    };
    for (taxon_id, rank, names) in target
        .breadcrumb
        .iter()
        .map(|item| (item.taxon_id, item.rank, &item.names))
        .chain(std::iter::once((
            target.taxon_id,
            target.rank,
            &target.names,
        )))
    {
        let Some(expected_name) = input.path[rank.index()].as_deref() else {
            continue;
        };
        if names.sci_name.as_deref() == Some(expected_name) {
            update_name_fields(
                transaction,
                taxon_id,
                TaxonomyNameType::SciName,
                expected_name,
                None,
                Some(source),
                changes,
            )?;
        }
    }
    Ok(())
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
        params![taxon_id, accepted_type.as_str()],
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
        params![taxon_id, name_type.as_str(), name, authority_year, source],
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
            params![taxon_id, name_type.as_str(), name],
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
            params![authority_year, taxon_id, name_type.as_str(), name],
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
            params![source, taxon_id, name_type.as_str(), name],
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
            SELECT name_type FROM taxon_names
            WHERE taxon_id = ? AND name = ? AND name_type IN (?, ?)
            ORDER BY CASE name_type WHEN ? THEN 0 ELSE 1 END
            LIMIT 1
            "#,
            params![
                taxon_id,
                name,
                accepted_type.as_str(),
                alias_type.as_str(),
                accepted_type.as_str()
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|value| TaxonomyNameType::from_value(&value))
        .transpose()
}

fn existing_scientific_name_type(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    name: &str,
) -> CoreResult<Option<TaxonomyNameType>> {
    transaction
        .query_row(
            r#"
            SELECT name_type FROM taxon_names
            WHERE taxon_id = ? AND name = ?
              AND name_type IN ('sci_name', 'synonym')
            ORDER BY CASE name_type WHEN 'sci_name' THEN 0 ELSE 1 END
            LIMIT 1
            "#,
            params![taxon_id, name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|value| TaxonomyNameType::from_value(&value))
        .transpose()
}

fn find_target(transaction: &Transaction<'_>, input: &NormalizedInput) -> CoreResult<MatchResult> {
    let candidates = find_scientific_candidates(
        transaction,
        input.target_rank,
        &input.target_name,
        input,
        input.target_rank,
    )?;
    if !candidates.is_empty() {
        return Ok(match candidates.as_slice() {
            [one] => MatchResult::One(one.clone()),
            _ => MatchResult::Many(candidates),
        });
    }
    for synonym in &input.synonyms {
        let candidates =
            find_name_candidates(transaction, input.target_rank, &synonym.name, input)?;
        if !candidates.is_empty() {
            return Ok(match candidates.as_slice() {
                [one] => MatchResult::One(one.clone()),
                _ => MatchResult::Many(candidates),
            });
        }
    }
    Ok(MatchResult::None)
}

fn find_scientific_candidates(
    transaction: &Transaction<'_>,
    rank: TaxonRank,
    name: &str,
    input: &NormalizedInput,
    lineage_limit: TaxonRank,
) -> CoreResult<Vec<TaxonSummary>> {
    find_candidates_by_type(
        transaction,
        rank,
        name,
        Some(TaxonomyNameType::SciName),
        input,
        lineage_limit,
    )
}

fn find_name_candidates(
    transaction: &Transaction<'_>,
    rank: TaxonRank,
    name: &str,
    input: &NormalizedInput,
) -> CoreResult<Vec<TaxonSummary>> {
    find_candidates_by_type(transaction, rank, name, None, input, rank)
}

fn find_candidates_by_type(
    transaction: &Transaction<'_>,
    rank: TaxonRank,
    name: &str,
    name_type: Option<TaxonomyNameType>,
    input: &NormalizedInput,
    lineage_limit: TaxonRank,
) -> CoreResult<Vec<TaxonSummary>> {
    let mut statement = if name_type.is_some() {
        transaction.prepare(
            r#"
            SELECT DISTINCT taxa.taxon_id
            FROM taxa JOIN taxon_names USING (taxon_id)
            WHERE taxa.rank = ? AND taxon_names.name = ? COLLATE BINARY
              AND taxon_names.name_type = ?
            ORDER BY taxa.taxon_id
            "#,
        )?
    } else {
        transaction.prepare(
            r#"
            SELECT DISTINCT taxa.taxon_id
            FROM taxa JOIN taxon_names USING (taxon_id)
            WHERE taxa.rank = ? AND taxon_names.name = ? COLLATE BINARY
              AND taxon_names.name_type IN ('sci_name', 'synonym')
            ORDER BY taxa.taxon_id
            "#,
        )?
    };
    let ids = if let Some(name_type) = name_type {
        statement
            .query_map(params![rank.code(), name, name_type.as_str()], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        statement
            .query_map(params![rank.code(), name], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut filtered = Vec::new();
    for taxon_id in ids {
        let mut matches = true;
        for ancestor_rank in TaxonRank::ALL.into_iter().take(lineage_limit.index()) {
            if let Some(expected) = input.path[ancestor_rank.index()].as_deref()
                && !lineage_has_scientific_name(transaction, taxon_id, ancestor_rank, expected)?
            {
                matches = false;
                break;
            }
        }
        if matches {
            filtered.push(taxon_id);
        }
    }
    load_taxon_summaries(transaction, &filtered)
}

fn lineage_has_scientific_name(
    transaction: &Transaction<'_>,
    taxon_id: i64,
    rank: TaxonRank,
    name: &str,
) -> CoreResult<bool> {
    Ok(transaction.query_row(
        r#"
        WITH RECURSIVE lineage(taxon_id, parent_taxon_id, rank) AS (
            SELECT taxon_id, parent_taxon_id, rank FROM taxa WHERE taxon_id = ?
            UNION ALL
            SELECT parent.taxon_id, parent.parent_taxon_id, parent.rank
            FROM taxa AS parent JOIN lineage AS child
              ON child.parent_taxon_id = parent.taxon_id
        )
        SELECT EXISTS(
            SELECT 1 FROM lineage JOIN taxon_names USING (taxon_id)
            WHERE lineage.rank = ? AND taxon_names.name_type = 'sci_name'
              AND taxon_names.name = ? COLLATE BINARY
        )
        "#,
        params![taxon_id, rank.code(), name],
        |row| row.get(0),
    )?)
}

enum MatchResult {
    None,
    One(TaxonSummary),
    Many(Vec<TaxonSummary>),
}

#[derive(Debug)]
struct ParsedSynonym {
    name: String,
    authority_year: Option<String>,
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
    fn from_row(row: &TaxonInputRow) -> Result<Self, String> {
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
        for index in 1..target_index {
            if path[index].is_some() && path[index - 1].is_none() {
                return Err(format!(
                    "{} cannot be supplied without {}",
                    TaxonRank::ALL[index].as_str(),
                    TaxonRank::ALL[index - 1].as_str()
                ));
            }
        }
        let target_name = path[target_index].clone().unwrap_or_default();
        let synonyms = unique_names(&row.synonyms)
            .into_iter()
            .filter_map(|raw| {
                let parts = split_scientific_name_authority(&raw);
                (!parts.name.is_empty()).then_some(ParsedSynonym {
                    name: parts.name,
                    authority_year: normalize_text(parts.authority_year.as_deref()),
                })
            })
            .collect();
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

    fn parent_name(&self) -> Option<&str> {
        self.target_rank
            .parent()
            .and_then(|rank| self.path[rank.index()].as_deref())
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
    value.and_then(|value| {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        (!value.is_empty()).then_some(value)
    })
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
    database.connect()?.query_row(
        "SELECT metadata_value FROM app_metadata WHERE metadata_key = 'taxonomy_name_separator'",
        [],
        |row| row.get(0),
    ).map_err(Into::into)
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
    database.connect()?.execute(
        r#"
        INSERT INTO app_metadata(metadata_key, metadata_value)
        VALUES ('taxonomy_name_separator', ?)
        ON CONFLICT(metadata_key) DO UPDATE SET metadata_value = excluded.metadata_value
        "#,
        [separator],
    )?;
    Ok(())
}

pub fn taxonomy_formatted_update_template() -> CoreResult<String> {
    let mut writer = WriterBuilder::new().delimiter(b'|').from_writer(Vec::new());
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
        .delimiter(b'|')
        .flexible(true)
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
            let multiple = || {
                value
                    .split(separator)
                    .filter_map(|value| normalize_name(Some(value)))
                    .collect::<Vec<_>>()
            };
            match header {
                "kingdom" => row.kingdom = scalar(),
                "order" => row.order = scalar(),
                "family" => row.family = scalar(),
                "genus" => row.genus = scalar(),
                "species" => row.species = scalar(),
                "authority_year" => row.authority_year = scalar(),
                "synonyms" => row.synonyms = multiple(),
                "zh_name" => row.zh_name = scalar(),
                "zh_alias" => row.zh_alias = multiple(),
                "en_name" => row.en_name = scalar(),
                "en_alias" => row.en_alias = multiple(),
                "geological_range" => row.geological_range = scalar(),
                "source" => row.source = scalar(),
                _ => {}
            }
        }
        rows.push(row);
    }
    Ok(rows)
}

pub fn taxonomy_log_csv(rows: &[TaxonRowOutcome]) -> CoreResult<String> {
    let mut writer = WriterBuilder::new().delimiter(b'|').from_writer(Vec::new());
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

pub fn list_taxonomy_operations(
    database: &Database,
    cursor: Option<&str>,
    limit: usize,
) -> CoreResult<TaxonomyPage<TaxonomyOperation>> {
    let before_id = match decode_cursor(cursor)? {
        Some(TaxonomyCursor::Operations { operation_id }) => Some(operation_id),
        Some(_) => return Err(invalid_cursor()),
        None => None,
    };
    let connection = database.connect()?;
    let limit = page_limit(limit);
    let mut items = if let Some(operation_id) = before_id {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, source, input_json, result_json,
                   length(changeset_blob), applied_at
            FROM taxonomy_operations
            WHERE operation_id < ?
            ORDER BY operation_id DESC LIMIT ?
            "#,
        )?;
        statement
            .query_map(params![operation_id, (limit + 1) as i64], operation_row)?
            .map(operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    } else {
        let mut statement = connection.prepare(
            r#"
            SELECT operation_id, source, input_json, result_json,
                   length(changeset_blob), applied_at
            FROM taxonomy_operations
            ORDER BY operation_id DESC LIMIT ?
            "#,
        )?;
        statement
            .query_map([(limit + 1) as i64], operation_row)?
            .map(operation_from_row)
            .collect::<CoreResult<Vec<_>>>()?
    };
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if has_more {
        items
            .last()
            .map(|operation| {
                encode_cursor(&TaxonomyCursor::Operations {
                    operation_id: operation.operation_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    Ok(TaxonomyPage { items, next_cursor })
}

pub fn get_taxonomy_operation(
    database: &Database,
    operation_id: i64,
) -> CoreResult<Option<TaxonomyOperation>> {
    database
        .connect()?
        .query_row(
            r#"
            SELECT operation_id, source, input_json, result_json,
                   length(changeset_blob), applied_at
            FROM taxonomy_operations WHERE operation_id = ?
            "#,
            [operation_id],
            operation_row,
        )
        .optional()?
        .map(|row| operation_from_row(Ok(row)))
        .transpose()
}

type StoredOperationRow = (i64, String, String, String, i64, String);

fn operation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredOperationRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn operation_from_row(row: rusqlite::Result<StoredOperationRow>) -> CoreResult<TaxonomyOperation> {
    let (operation_id, source, input, result, changeset_size, applied_at) = row?;
    Ok(TaxonomyOperation {
        operation_id,
        source: TaxonomyOperationSource::from_str(&source)?,
        input: deserialize_json(&input, "taxonomy operation input")?,
        result: deserialize_json(&result, "taxonomy operation result")?,
        changeset_size: changeset_size as usize,
        applied_at,
    })
}

pub fn export_taxonomy_operation_inputs(
    database: &Database,
    operation_ids: &[i64],
) -> CoreResult<OperationInputTable> {
    let ids = unique_operation_ids(operation_ids)?;
    let separator = get_taxonomy_name_separator(database)?;
    let connection = database.connect()?;
    let mut output = Vec::new();
    if !ids.is_empty() {
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = connection.prepare(&format!(
            "SELECT input_json FROM taxonomy_operations \
             WHERE operation_id IN ({placeholders}) ORDER BY operation_id"
        ))?;
        let input_json = statement
            .query_map(params_from_iter(ids.iter()), |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if input_json.len() != ids.len() {
            return Err(CoreError::NotFound(
                "one or more taxonomy operations".into(),
            ));
        }
        for value in input_json {
            let rows: Vec<TaxonInputRow> = deserialize_json(&value, "taxonomy operation input")?;
            output.extend(rows.into_iter().map(|row| input_table_row(row, &separator)));
        }
    }
    Ok(OperationInputTable {
        columns: TAXONOMY_INPUT_COLUMNS
            .into_iter()
            .map(str::to_string)
            .collect(),
        rows: output,
    })
}

fn input_table_row(row: TaxonInputRow, separator: &str) -> Vec<String> {
    vec![
        row.kingdom.unwrap_or_default(),
        row.order.unwrap_or_default(),
        row.family.unwrap_or_default(),
        row.genus.unwrap_or_default(),
        row.species.unwrap_or_default(),
        row.authority_year.unwrap_or_default(),
        row.synonyms.join(separator),
        row.zh_name.unwrap_or_default(),
        row.zh_alias.join(separator),
        row.en_name.unwrap_or_default(),
        row.en_alias.join(separator),
        row.geological_range.unwrap_or_default(),
        row.source.unwrap_or_default(),
    ]
}

fn unique_operation_ids(operation_ids: &[i64]) -> CoreResult<Vec<i64>> {
    let ids = operation_ids.iter().copied().collect::<BTreeSet<_>>();
    if ids.iter().any(|operation_id| *operation_id <= 0) {
        return Err(CoreError::InvalidArgument(
            "operation ids must be positive".into(),
        ));
    }
    Ok(ids.into_iter().collect())
}

pub fn revert_taxonomy_operation(database: &Database, operation_id: i64) -> CoreResult<()> {
    let mut connection = database.connect()?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changeset_blob = transaction
        .query_row(
            "SELECT changeset_blob FROM taxonomy_operations WHERE operation_id = ?",
            [operation_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("taxonomy operation {operation_id}")))?;
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
    transaction.execute(
        "DELETE FROM taxonomy_operations WHERE operation_id = ?",
        [operation_id],
    )?;
    transaction.commit()?;
    mapping::refresh_after_taxonomy_changes(database, affected_taxon_ids)?;
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
        Err(rusqlite::Error::InvalidColumnIndex(_)) => Ok(false),
        Err(error) => Err(error.into()),
        Ok(_) => Err(CoreError::Consistency(
            "taxonomy changeset identifier is not an integer".into(),
        )),
    }
}

pub(super) fn validate_taxonomy(connection: &Connection) -> CoreResult<()> {
    let invalid_taxon = connection
        .query_row(
            r#"
            SELECT child.taxon_id
            FROM taxa AS child
            LEFT JOIN taxa AS parent ON parent.taxon_id = child.parent_taxon_id
            WHERE (child.rank = 1 AND child.parent_taxon_id IS NOT NULL)
               OR (child.rank > 1 AND (parent.taxon_id IS NULL OR parent.rank != child.rank - 1))
            LIMIT 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(taxon_id) = invalid_taxon {
        return Err(CoreError::InvalidArgument(format!(
            "taxon {taxon_id} has invalid parentage"
        )));
    }
    let invalid_name = connection
        .query_row(
            r#"
            SELECT taxa.taxon_id
            FROM taxa
            LEFT JOIN taxon_names
              ON taxon_names.taxon_id = taxa.taxon_id
             AND taxon_names.name_type = 'sci_name'
            GROUP BY taxa.taxon_id
            HAVING COUNT(taxon_names.name_id) != 1
            LIMIT 1
            "#,
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(taxon_id) = invalid_name {
        return Err(CoreError::InvalidArgument(format!(
            "taxon {taxon_id} must have exactly one sci_name"
        )));
    }
    let mut statement = connection.prepare("SELECT name_id, name FROM taxon_names")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })? {
        let (name_id, name) = row?;
        if normalize_name(Some(&name)).as_deref() != Some(name.as_str()) {
            return Err(CoreError::InvalidArgument(format!(
                "taxon name {name_id} is not normalized"
            )));
        }
    }
    Ok(())
}

pub(super) fn serialize_json<T: Serialize + ?Sized>(value: &T, label: &str) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid {label}: {error}")))
}

fn deserialize_json<T: for<'de> Deserialize<'de>>(value: &str, label: &str) -> CoreResult<T> {
    serde_json::from_str(value)
        .map_err(|error| CoreError::InvalidArgument(format!("invalid {label}: {error}")))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn database() -> (TempDir, Database) {
        let directory = TempDir::new().unwrap();
        let database = Database::open(directory.path().join("test.db")).unwrap();
        (directory, database)
    }

    #[test]
    fn csv_accepts_subset_and_reordered_headers() {
        let (_directory, database) = database();
        let rows = parse_taxonomy_input_csv(
            &database,
            "synonyms|species|zh_alias\nCanis lycaon Linnaeus, 1758|Canis lupus|wolf;dog\n",
        )
        .unwrap();
        assert_eq!(rows[0].species.as_deref(), Some("Canis lupus"));
        assert_eq!(rows[0].synonyms.len(), 1);
        assert_eq!(rows[0].zh_alias, vec!["wolf", "dog"]);
    }

    #[test]
    fn preview_rolls_back_and_apply_is_revertible() {
        let (_directory, database) = database();
        let input = TaxonInputRow {
            kingdom: Some("Animalia".into()),
            ..TaxonInputRow::default()
        };
        assert_eq!(
            preview_rows(&database, std::slice::from_ref(&input))
                .unwrap()
                .rows[0]
                .operation_types,
            vec![TaxonRowStatus::NewTaxon, TaxonRowStatus::NewName]
        );
        assert!(
            database
                .connect()
                .unwrap()
                .query_row("SELECT NOT EXISTS(SELECT 1 FROM taxa)", [], |row| row
                    .get::<_, bool>(0),)
                .unwrap()
        );
        let result = apply_rows(&database, &[input]).unwrap();
        revert_taxonomy_operation(&database, result.operation_id).unwrap();
        assert!(
            get_taxonomy_operation(&database, result.operation_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn one_row_reports_every_change_type() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                authority_year: Some("old".into()),
                geological_range: Some("old".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "UPDATE taxon_names SET source = NULL WHERE name_type = 'sci_name'",
                [],
            )
            .unwrap();
        drop(connection);

        let preview = preview_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                authority_year: Some("new".into()),
                synonyms: vec!["Metazoa Linnaeus, 1758".into()],
                geological_range: Some("new".into()),
                source: Some("catalog".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();

        assert_eq!(
            preview.rows[0].operation_types,
            vec![
                TaxonRowStatus::Supplement,
                TaxonRowStatus::NewName,
                TaxonRowStatus::Overwrite
            ]
        );
    }

    #[test]
    fn source_only_fills_an_empty_existing_value() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                source: Some("first".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        let result = apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                source: Some("second".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        assert_eq!(
            result.rows[0].operation_types,
            vec![TaxonRowStatus::NoChange]
        );
        let source: String = database
            .connect()
            .unwrap()
            .query_row(
                "SELECT source FROM taxon_names WHERE name_type = 'sci_name'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "first");
    }

    #[test]
    fn row_source_applies_to_supplied_lineage_names() {
        let (_directory, database) = database();
        apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        let result = apply_rows(
            &database,
            &[TaxonInputRow {
                kingdom: Some("Animalia".into()),
                order: Some("Carnivora".into()),
                source: Some("catalog".into()),
                ..TaxonInputRow::default()
            }],
        )
        .unwrap();
        assert_eq!(
            result.rows[0].operation_types,
            vec![
                TaxonRowStatus::NewTaxon,
                TaxonRowStatus::Supplement,
                TaxonRowStatus::NewName
            ]
        );
        let source: String = database
            .connect()
            .unwrap()
            .query_row(
                "SELECT source FROM taxon_names WHERE name = 'Animalia'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "catalog");
    }
}
