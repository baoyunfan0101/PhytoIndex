use std::path::Path;

use phytoindex_core::mapping::{
    PhotoMappingListItem, PhotoMappingListStatus, PhotoTaxonItem, PhotoTaxonMapping,
    PhotoTaxonMatch, PhotoTaxonNode,
};
use phytoindex_core::models::{
    DirectoryEntryCounts, MappingMetadata, OperationsStatus, Photo, PhotoDirectoryItem,
    PhotoLibrary, PhotoMetadata, PhotoPage,
};
use phytoindex_core::photos::{PhotoOperation, PhotoOperationBatch, PhotoRenameBatchResult};
use phytoindex_core::taxonomy::{
    DeleteTaxonNameInput, TaxonChild, TaxonDetailNode, TaxonSearchResult, TaxonUpdateInput,
    TaxonUpdateOptions, TaxonomyActionResult, TaxonomyCustomSqlResult, TaxonomyCustomSqlTempTable,
    TaxonomyOperation, TaxonomyOperationBatch, TaxonomyPage, TaxonomyUpdateActionResult,
};
use phytoindex_core::{export, mapping, photos, taxonomy};
use serde_json::{Value, json};
use tauri::{AppHandle, State};

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_photo_library(state: State<'_, AppState>) -> CommandResult<Option<PhotoLibrary>> {
    photos::get_library(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_photo_library_count(state: State<'_, AppState>) -> CommandResult<i64> {
    photos::get_photo_count(&state.database).map_err(error)
}

#[tauri::command]
pub fn open_photo_library(state: State<'_, AppState>, root: String) -> CommandResult<PhotoLibrary> {
    photos::open_library(&state.database, &root).map_err(error)
}

#[tauri::command]
pub fn browse_photo_directory(
    state: State<'_, AppState>,
    directory_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoDirectoryItem>> {
    photos::browse_directory(
        &state.database,
        directory_id,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn get_photo_directory_counts(
    state: State<'_, AppState>,
    directory_id: i64,
) -> CommandResult<DirectoryEntryCounts> {
    photos::get_directory_counts(&state.database, directory_id).map_err(error)
}

#[tauri::command]
pub fn refresh_photo_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    directory_id: i64,
) -> CommandResult<Value> {
    let database = state.database.clone();
    let operation = state
        .operations
        .start(app, "photos", "refresh", move |progress| {
            progress(0, None, "Refreshing directory");
            let refresh = photos::refresh_directory(&database, directory_id).map_err(error)?;
            let mapping =
                mapping::process_pending_photo_matches(&database, progress).map_err(error)?;
            Ok(json!({ "refresh": refresh, "mapping": mapping }))
        })?;
    Ok(json!({ "operation": operation }))
}

#[tauri::command]
pub fn start_photo_mapping(app: AppHandle, state: State<'_, AppState>) -> CommandResult<Value> {
    let database = state.database.clone();
    let operation = state
        .operations
        .start(app, "mapping", "match", move |progress| {
            let result =
                mapping::process_pending_photo_matches(&database, progress).map_err(error)?;
            serde_json::to_value(result).map_err(error)
        })?;
    Ok(json!({ "operation": operation }))
}

#[tauri::command]
pub fn rename_photo(
    state: State<'_, AppState>,
    photo_id: i64,
    new_filename: String,
) -> CommandResult<Photo> {
    photos::rename_photo(&state.database, photo_id, &new_filename).map_err(error)
}

#[tauri::command]
pub fn rename_photo_from_taxon(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Photo> {
    photos::rename_photo_from_taxon(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn rename_photos_from_taxa(
    state: State<'_, AppState>,
    photo_ids: Vec<i64>,
) -> CommandResult<PhotoRenameBatchResult> {
    photos::rename_photos_from_taxa(&state.database, &photo_ids).map_err(error)
}

#[tauri::command]
pub fn list_photo_operation_batches(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoOperationBatch>> {
    photos::list_photo_operation_batches(&state.database, cursor.as_deref(), limit.unwrap_or(50))
        .map_err(error)
}

#[tauri::command]
pub fn list_photo_operations(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoOperation>> {
    photos::list_photo_operations(&state.database, cursor.as_deref(), limit.unwrap_or(50))
        .map_err(error)
}

#[tauri::command]
pub fn list_photo_operations_for_batch(
    state: State<'_, AppState>,
    batch_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoOperation>> {
    photos::list_photo_operations_for_batch(
        &state.database,
        batch_id,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn revert_photo_operation(state: State<'_, AppState>, operation_id: i64) -> CommandResult<()> {
    photos::revert_photo_operation(&state.database, operation_id).map_err(error)
}

#[tauri::command]
pub fn get_all_photos(state: State<'_, AppState>) -> CommandResult<Vec<Photo>> {
    photos::list_photos(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_photo(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Photo> {
    photos::get_photo(&state.database, photo_id)
        .map_err(error)?
        .ok_or_else(|| format!("photo {photo_id} not found"))
}

#[tauri::command]
pub fn get_photo_availability(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Value> {
    Ok(match photos::photo_file_path(&state.database, photo_id) {
        Ok(_) => json!({ "available": true, "error": null }),
        Err(error) => json!({ "available": false, "error": error.to_string() }),
    })
}

#[tauri::command]
pub fn get_photo_metadata(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoMetadata> {
    photos::get_photo_metadata(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn search_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSearchResult>> {
    taxonomy::search_taxa(&state.database, &query, limit.unwrap_or(50)).map_err(error)
}

#[tauri::command]
pub fn get_taxon_detail_node(
    state: State<'_, AppState>,
    taxon_id: i64,
    children_cursor: Option<String>,
    children_limit: Option<usize>,
) -> CommandResult<TaxonDetailNode> {
    taxonomy::get_taxon_detail_node(
        &state.database,
        taxon_id,
        children_cursor.as_deref(),
        children_limit.unwrap_or(50),
    )
    .map_err(error)?
    .ok_or_else(|| format!("taxon {taxon_id} not found"))
}

#[tauri::command]
pub fn list_taxon_children(
    state: State<'_, AppState>,
    taxon_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<TaxonomyPage<TaxonChild>> {
    taxonomy::list_taxon_children(
        &state.database,
        taxon_id,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn delete_taxon_name(
    state: State<'_, AppState>,
    input: DeleteTaxonNameInput,
) -> CommandResult<TaxonomyActionResult> {
    taxonomy::delete_taxon_name(&state.database, input).map_err(error)
}

#[tauri::command]
pub fn update_taxon(
    state: State<'_, AppState>,
    input: TaxonUpdateInput,
    options: Option<TaxonUpdateOptions>,
) -> CommandResult<TaxonomyUpdateActionResult> {
    taxonomy::update_taxon(&state.database, input, options.unwrap_or_default()).map_err(error)
}

#[tauri::command]
pub fn delete_taxon(
    state: State<'_, AppState>,
    taxon_id: i64,
) -> CommandResult<TaxonomyActionResult> {
    taxonomy::delete_taxon(&state.database, taxon_id).map_err(error)
}

#[tauri::command]
pub fn execute_custom_taxonomy_sql(
    state: State<'_, AppState>,
    sql: String,
    input: Option<TaxonomyCustomSqlTempTable>,
) -> CommandResult<TaxonomyCustomSqlResult> {
    taxonomy::execute_custom_taxonomy_sql(&state.database, &sql, input).map_err(error)
}

#[tauri::command]
pub fn list_taxonomy_operation_batches(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<TaxonomyPage<TaxonomyOperationBatch>> {
    taxonomy::list_taxonomy_operation_batches(
        &state.database,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn list_taxonomy_operations(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<TaxonomyPage<TaxonomyOperation>> {
    taxonomy::list_taxonomy_operations(&state.database, cursor.as_deref(), limit.unwrap_or(50))
        .map_err(error)
}

#[tauri::command]
pub fn list_taxonomy_operations_for_batch(
    state: State<'_, AppState>,
    batch_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<TaxonomyPage<TaxonomyOperation>> {
    taxonomy::list_taxonomy_operations_for_batch(
        &state.database,
        batch_id,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn get_mapping_metadata(state: State<'_, AppState>) -> CommandResult<MappingMetadata> {
    mapping::get_metadata(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_photo_taxon_match(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoTaxonMatch> {
    mapping::get_photo_taxon_match(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn select_photo_taxon(
    state: State<'_, AppState>,
    photo_id: i64,
    taxon_id: i64,
) -> CommandResult<PhotoTaxonMapping> {
    mapping::select_photo_taxon(&state.database, photo_id, taxon_id).map_err(error)
}

#[tauri::command]
pub fn get_photo_taxon_node(
    state: State<'_, AppState>,
    taxon_id: Option<i64>,
    show_empty: Option<bool>,
) -> CommandResult<PhotoTaxonNode> {
    mapping::get_photo_taxon_node(&state.database, taxon_id, show_empty.unwrap_or(false))
        .map_err(error)
}

#[tauri::command]
pub fn browse_photo_taxon(
    state: State<'_, AppState>,
    taxon_id: Option<i64>,
    show_empty: Option<bool>,
    include_descendants: Option<bool>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoTaxonItem>> {
    mapping::browse_photo_taxon(
        &state.database,
        taxon_id,
        show_empty.unwrap_or(false),
        include_descendants.unwrap_or(true),
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn list_photos_by_mapping_status(
    state: State<'_, AppState>,
    status: PhotoMappingListStatus,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoMappingListItem>> {
    mapping::list_photos_by_mapping_status(
        &state.database,
        status,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn suggest_mapping_taxa(
    state: State<'_, AppState>,
    query: String,
    mode: String,
) -> CommandResult<Vec<TaxonSearchResult>> {
    mapping::suggest(&state.database, &query, &mode, 10).map_err(error)
}

#[tauri::command]
pub fn get_operations_status(state: State<'_, AppState>) -> OperationsStatus {
    state.operations.status()
}

#[tauri::command]
pub fn export_table(
    state: State<'_, AppState>,
    table_name: String,
    output_path: String,
) -> CommandResult<Value> {
    let exported = export::export_table(&state.database, &table_name, Path::new(&output_path))
        .map_err(error)?;
    Ok(json!({ "exported": exported, "output_path": output_path }))
}

fn error(error: impl ToString) -> String {
    error.to_string()
}
