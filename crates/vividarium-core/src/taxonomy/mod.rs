//! Taxonomy search, detail views, mutations, import workflows, and history.
//!
//! This module is the public taxonomy facade. SQL execution details, session
//! storage, validation, and synchronization remain private implementation
//! details.

mod actions;
mod cleanup;
mod direct_import;
mod formatted;
mod operation_export;
mod page;
mod query;
mod sql;
mod sql_import;
mod sql_inputs;
mod sql_support;
pub(crate) mod sync;
mod view;

pub use crate::naming::{ScientificNameParts, split_scientific_name_authority};
pub use actions::{
    DeleteTaxonNameInput, NewTaxonNameInput, PromoteTaxonNameInput, SaveTaxonNameGroupInput,
    TaxonNameMetadataInput, TaxonUpdateInput, delete_taxon, delete_taxon_name, promote_taxon_name,
    save_taxon_name_group, update_taxon,
};
pub use direct_import::{
    DirectImportDatabase, TaxonomyImportMetadata, TaxonomyImportResult, apply_direct_import,
    get_taxonomy_import_metadata, inspect_direct_import_database,
};
pub use formatted::{
    PreparedTaxonomyUpdate, TaxonChange, TaxonChangeKind, TaxonInputRow, TaxonRank,
    TaxonRowOutcome, TaxonRowStatus, TaxonomyNameType, TaxonomyOperationResult,
    TaxonomyPreviewResult, apply_prepared_rows, apply_rows, get_taxonomy_name_separator,
    list_operation_audit, list_operations, parse_taxonomy_input_csv, prepare_rows, preview_rows,
    rollback_operation, set_taxonomy_name_separator, taxonomy_formatted_update_template,
    taxonomy_log_csv,
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
    export_custom_taxonomy_query, get_custom_taxonomy_sql, list_custom_sql_database_schemas,
    list_custom_sql_inputs, remove_custom_sql_input,
};
pub use sql_import::{
    NameTypeCount, SqlImportExecutionResult, SqlImportIssue, SqlImportValidationResult,
    ValidateSqlImportRequest, ValidateSqlImportResult, add_sql_import_input, apply_sql_import,
    get_sql_import_sql, list_sql_import_database_schemas, list_sql_import_inputs,
    list_sql_import_staging_schemas, remove_sql_import_input, validate_sql_import,
    validate_sql_import_with_progress,
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
