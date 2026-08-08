//! Taxonomy search, detail views, mutations, import workflows, and history.
//!
//! This module is the public taxonomy facade. SQL execution details, session
//! storage, validation, and synchronization remain private implementation
//! details.

mod actions;
mod base;
mod base_import;
mod cleanup;
mod formatted;
mod operation_export;
mod page;
mod query;
mod sql;
mod sql_inputs;
mod sql_support;
pub(crate) mod sync;
mod view;

pub use crate::naming::{ScientificNameParts, split_scientific_name_authority};
pub use actions::{
    DeleteTaxonNameInput, PromoteTaxonNameInput, TaxonUpdateInput, delete_taxon, delete_taxon_name,
    promote_taxon_name, update_taxon,
};
pub use base::{
    TaxonomyBaseMetadata, TaxonomyBaseReplaceResult, get_taxonomy_base_metadata,
    replace_taxonomy_base_database,
};
pub use base_import::{
    BaseImportExecutionResult, BaseImportIssue, BaseImportValidationResult, NameTypeCount,
    ValidateBaseImportRequest, ValidateBaseImportResult, add_base_import_input, apply_base_import,
    get_base_import_sql, list_base_import_inputs, remove_base_import_input, validate_base_import,
    validate_base_import_with_progress,
};
pub use formatted::{
    TaxonChange, TaxonChangeKind, TaxonInputRow, TaxonRank, TaxonRowOutcome, TaxonRowStatus,
    TaxonomyNameType, TaxonomyOperationResult, TaxonomyPreviewResult, apply_rows,
    get_taxonomy_name_separator, list_operation_audit, list_operations, parse_taxonomy_input_csv,
    preview_rows, rollback_operation, set_taxonomy_name_separator,
    taxonomy_formatted_update_template, taxonomy_log_csv,
};
pub use operation_export::{
    export_all_replayable_inputs, export_operation_input, export_operations_input,
    write_all_operation_audit, write_operation_audit, write_operations_audit,
};
pub use page::TaxonomyPage;
pub use query::{TaxonNameMatch, TaxonSearchResult, TaxonSuggestion, search_taxa, suggest_taxa};
pub(crate) use query::{
    TaxonSearchCursorKey, search_taxa_page_with_photos_connection,
    suggest_taxa_with_photos_connection, taxon_search_relation,
};
pub use sql::{
    CustomSqlExecutionResult, CustomTaxonomySqlExportRequest, CustomTaxonomySqlRequest, SqlColumn,
    SqlExportResult, SqlObjectType, SqlResultSet, SqlSourceObject, SqlSourceSchema,
    SqlStatementMessage, SqlValue, add_custom_sql_input, execute_custom_taxonomy_sql,
    export_custom_taxonomy_query, get_custom_taxonomy_sql, list_custom_sql_inputs,
    remove_custom_sql_input,
};
pub use sql_inputs::{
    AddSqlInputRequest, AddSqlInputResult, PersistentSqlInput, RemoveSqlInputRequest,
    RemoveSqlInputResult, SqlInputKind,
};
pub use sync::{TaxonomySyncResult, TaxonomySyncRun, synchronize_pending_photo_libraries};
pub(crate) use view::load_taxon_summaries;
pub use view::{
    TaxonBreadcrumbItem, TaxonChild, TaxonDetail, TaxonDisplayNames, TaxonNameDetail,
    TaxonNamesDetail, TaxonSummary, get_taxon_detail, get_taxon_summary, list_taxon_children,
};
