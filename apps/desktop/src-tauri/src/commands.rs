use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, State, ipc::Channel};
use vividarium_core::mapping::{
    PhotoMappingListItem, PhotoMappingListStatus, PhotoNameMatchSettings, PhotoTaxonItem,
    PhotoTaxonMapping, PhotoTaxonMatch, PhotoTaxonNode,
};
use vividarium_core::models::{
    DirectoryEntryCounts, MappingMetadata, OperationsStatus, Photo, PhotoDirectoryItem,
    PhotoLibrary, PhotoMetadata, PhotoPage,
};
use vividarium_core::naming::{
    NamingHookKind, NamingHookSettings, NamingHookTemplates, NamingHookTestCase,
    NamingHookTestCases, NamingHookTestReport, NamingHookTestResult, ParsedPhotoFilename,
    TaxonomicNameInfo,
};
use vividarium_core::photos::{
    PhotoFilenameFormatSettings, PhotoOperation, PhotoRenameOperationResult,
};
use vividarium_core::taxonomy::{
    DeleteTaxonNameInput, PromoteTaxonNameInput, TaxonChild, TaxonDetailNode, TaxonInputRow,
    TaxonRowOutcome, TaxonSearchResult, TaxonSuggestion, TaxonUpdateInput, TaxonomyBaseMetadata,
    TaxonomyCustomSqlResult, TaxonomyCustomSqlTempTable, TaxonomyOperation,
    TaxonomyOperationResult, TaxonomyPage, TaxonomyPreviewResult,
};
use vividarium_core::{
    map::{self, MapBounds, MapPhoto, MapSettings},
    mapping, naming, photos, taxonomy,
};

use crate::state::AppState;
use crate::updater::{AppUpdateEvent, AppUpdateInfo, PendingAppUpdate};

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    pending: State<'_, PendingAppUpdate>,
) -> CommandResult<Option<AppUpdateInfo>> {
    crate::updater::check(&app, pending.inner()).await
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingAppUpdate>,
    on_event: Channel<AppUpdateEvent>,
) -> CommandResult<()> {
    crate::updater::ensure_install_allowed(&state.operations.status())?;
    crate::updater::install(&app, pending.inner(), on_event).await
}

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
pub fn parse_photo_filename(
    state: State<'_, AppState>,
    filename: String,
) -> CommandResult<ParsedPhotoFilename> {
    naming::parse_photo_filename(&state.database, &filename).map_err(error)
}

#[tauri::command]
pub fn normalize_taxonomy_name(value: String) -> Option<String> {
    naming::normalize_taxonomy_name(&value)
}

#[tauri::command]
pub fn get_naming_hook_settings(state: State<'_, AppState>) -> CommandResult<NamingHookSettings> {
    naming::get_naming_hook_settings(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_naming_hook_templates() -> NamingHookTemplates {
    naming::get_naming_hook_templates()
}

#[tauri::command]
pub fn set_naming_hook(
    state: State<'_, AppState>,
    kind: NamingHookKind,
    script: Option<String>,
) -> CommandResult<()> {
    naming::set_naming_hook(&state.database, kind, script.as_deref()).map_err(error)
}

#[tauri::command]
pub fn test_naming_hook(
    kind: NamingHookKind,
    script: String,
    input: String,
) -> CommandResult<NamingHookTestResult> {
    naming::test_naming_hook(kind, &script, &input).map_err(error)
}

#[tauri::command]
pub fn get_naming_hook_test_cases(
    state: State<'_, AppState>,
) -> CommandResult<NamingHookTestCases> {
    naming::get_naming_hook_test_cases(&state.database).map_err(error)
}

#[tauri::command]
pub fn set_naming_hook_test_cases(
    state: State<'_, AppState>,
    kind: NamingHookKind,
    cases: Vec<NamingHookTestCase>,
) -> CommandResult<()> {
    naming::set_naming_hook_test_cases(&state.database, kind, &cases).map_err(error)
}

#[tauri::command]
pub fn run_naming_hook_tests(
    state: State<'_, AppState>,
    kind: NamingHookKind,
    script: Option<String>,
) -> CommandResult<NamingHookTestReport> {
    naming::run_naming_hook_tests(&state.database, kind, script.as_deref()).map_err(error)
}

#[tauri::command]
pub fn get_photo_name_match_settings(
    state: State<'_, AppState>,
) -> CommandResult<PhotoNameMatchSettings> {
    mapping::get_photo_name_match_settings(&state.database).map_err(error)
}

#[tauri::command]
pub fn set_photo_name_match_settings(
    state: State<'_, AppState>,
    settings: PhotoNameMatchSettings,
) -> CommandResult<()> {
    mapping::set_photo_name_match_settings(&state.database, &settings).map_err(error)
}

#[tauri::command]
pub fn get_photo_filename_format_settings(
    state: State<'_, AppState>,
) -> CommandResult<PhotoFilenameFormatSettings> {
    photos::get_photo_filename_format_settings(&state.database).map_err(error)
}

#[tauri::command]
pub fn set_photo_filename_format_settings(
    state: State<'_, AppState>,
    settings: PhotoFilenameFormatSettings,
) -> CommandResult<()> {
    photos::set_photo_filename_format_settings(&state.database, &settings).map_err(error)
}

#[tauri::command]
pub fn format_photo_filename(
    info: TaxonomicNameInfo,
    suffix: String,
    settings: PhotoFilenameFormatSettings,
) -> CommandResult<String> {
    photos::format_photo_filename(&info, &suffix, &settings).map_err(error)
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
) -> CommandResult<PhotoRenameOperationResult> {
    photos::rename_photos_from_taxa(&state.database, &photo_ids).map_err(error)
}

#[tauri::command]
pub fn rename_photos_in_directory_from_taxa(
    state: State<'_, AppState>,
    directory_id: i64,
    include_descendants: Option<bool>,
) -> CommandResult<PhotoRenameOperationResult> {
    photos::rename_photos_in_directory_from_taxa(
        &state.database,
        directory_id,
        include_descendants.unwrap_or(true),
    )
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
pub fn get_photo_operation(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<PhotoOperation> {
    photos::get_photo_operation(&state.database, operation_id)
        .map_err(error)?
        .ok_or_else(|| format!("photo operation {operation_id} not found"))
}

#[tauri::command]
pub fn revert_photo_operation(state: State<'_, AppState>, operation_id: i64) -> CommandResult<()> {
    photos::revert_photo_operation(&state.database, operation_id).map_err(error)
}

#[tauri::command]
pub fn export_photo_operation_csv(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<String> {
    photos::export_photo_operation_csv(&state.database, operation_id).map_err(error)
}

#[tauri::command]
pub fn export_all_photo_operations_csv(state: State<'_, AppState>) -> CommandResult<String> {
    photos::export_all_photo_operations_csv(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_photo(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Photo> {
    photos::get_photo(&state.database, photo_id)
        .map_err(error)?
        .ok_or_else(|| format!("photo {photo_id} not found"))
}

#[tauri::command]
pub fn search_photos(
    state: State<'_, AppState>,
    query: String,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<Photo>> {
    photos::search_photos(
        &state.database,
        &query,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn search_photos_by_filename(
    state: State<'_, AppState>,
    query: String,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<Photo>> {
    photos::search_photos_by_filename(
        &state.database,
        &query,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn get_photo_availability(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Value> {
    Ok(match photos::photo_file_path(&state.database, photo_id) {
        Ok(_) => json!({ "available": true, "error": null }),
        Err(error) => json!({ "available": false, "error": error.to_string() }),
    })
}

#[tauri::command]
pub fn reveal_photo_in_file_manager(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<()> {
    let path = photos::photo_file_path(&state.database, photo_id).map_err(error)?;
    crate::file_manager::reveal(&path)
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
pub fn suggest_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSuggestion>> {
    taxonomy::suggest_taxa(&state.database, &query, limit.unwrap_or(10)).map_err(error)
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
) -> CommandResult<()> {
    taxonomy::delete_taxon_name(&state.database, input).map_err(error)
}

#[tauri::command]
pub fn update_taxon(
    state: State<'_, AppState>,
    input: TaxonUpdateInput,
) -> CommandResult<TaxonomyOperationResult> {
    taxonomy::update_taxon(&state.database, input).map_err(error)
}

#[tauri::command]
pub fn promote_taxon_name(
    state: State<'_, AppState>,
    input: PromoteTaxonNameInput,
) -> CommandResult<()> {
    taxonomy::promote_taxon_name(&state.database, input).map_err(error)
}

#[tauri::command]
pub fn delete_taxon(state: State<'_, AppState>, taxon_id: i64) -> CommandResult<()> {
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
pub fn parse_custom_taxonomy_input_csv(input: String) -> CommandResult<TaxonomyCustomSqlTempTable> {
    taxonomy::parse_custom_taxonomy_input_csv(&input).map_err(error)
}

#[tauri::command]
pub fn preview_taxonomy_rows(
    state: State<'_, AppState>,
    rows: Vec<TaxonInputRow>,
) -> CommandResult<TaxonomyPreviewResult> {
    taxonomy::preview_rows(&state.database, &rows).map_err(error)
}

#[tauri::command]
pub fn apply_taxonomy_rows(
    state: State<'_, AppState>,
    rows: Vec<TaxonInputRow>,
) -> CommandResult<TaxonomyOperationResult> {
    taxonomy::apply_rows(&state.database, &rows).map_err(error)
}

#[tauri::command]
pub fn parse_taxonomy_input_csv(
    state: State<'_, AppState>,
    input: String,
) -> CommandResult<Vec<TaxonInputRow>> {
    taxonomy::parse_taxonomy_input_csv(&state.database, &input).map_err(error)
}

#[tauri::command]
pub fn get_taxonomy_formatted_update_template() -> CommandResult<String> {
    taxonomy::taxonomy_formatted_update_template().map_err(error)
}

#[tauri::command]
pub fn export_taxonomy_log(rows: Vec<TaxonRowOutcome>) -> CommandResult<String> {
    taxonomy::taxonomy_log_csv(&rows).map_err(error)
}

#[tauri::command]
pub fn get_taxonomy_name_separator(state: State<'_, AppState>) -> CommandResult<String> {
    taxonomy::get_taxonomy_name_separator(&state.database).map_err(error)
}

#[tauri::command]
pub fn set_taxonomy_name_separator(
    state: State<'_, AppState>,
    separator: String,
) -> CommandResult<()> {
    taxonomy::set_taxonomy_name_separator(&state.database, &separator).map_err(error)
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
pub fn get_taxonomy_operation(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<TaxonomyOperation> {
    taxonomy::get_taxonomy_operation(&state.database, operation_id)
        .map_err(error)?
        .ok_or_else(|| format!("taxonomy operation {operation_id} not found"))
}

#[tauri::command]
pub fn revert_taxonomy_operation(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<()> {
    taxonomy::revert_taxonomy_operation(&state.database, operation_id).map_err(error)
}

#[tauri::command]
pub fn export_taxonomy_operation_csv(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<String> {
    taxonomy::export_taxonomy_operation_csv(&state.database, operation_id).map_err(error)
}

#[tauri::command]
pub fn export_all_taxonomy_operations_csv(state: State<'_, AppState>) -> CommandResult<String> {
    taxonomy::export_all_taxonomy_operations_csv(&state.database).map_err(error)
}

#[tauri::command]
pub fn get_taxonomy_base_metadata(
    state: State<'_, AppState>,
) -> CommandResult<Option<TaxonomyBaseMetadata>> {
    taxonomy::get_taxonomy_base_metadata(&state.database).map_err(error)
}

#[tauri::command]
pub fn replace_taxonomy_base_database(
    app: AppHandle,
    state: State<'_, AppState>,
    source_path: String,
) -> CommandResult<Value> {
    let database = state.database.clone();
    let operation =
        state
            .operations
            .start(app, "mapping", "replace_taxonomy_base", move |progress| {
                progress(0, None, "Replacing taxonomy base database");
                let replacement =
                    taxonomy::replace_taxonomy_base_database(&database, Path::new(&source_path))
                        .map_err(error)?;
                progress(
                    0,
                    Some(replacement.queued_photo_count as u64),
                    "Remapping all photos",
                );
                let mapping =
                    mapping::process_pending_photo_matches(&database, progress).map_err(error)?;
                Ok(json!({ "replacement": replacement, "mapping": mapping }))
            })?;
    Ok(json!({ "operation": operation }))
}

#[tauri::command]
pub fn get_mapping_metadata(state: State<'_, AppState>) -> CommandResult<MappingMetadata> {
    mapping::get_metadata(&state.database).map_err(error)
}

#[tauri::command]
pub fn search_photo_taxa(
    state: State<'_, AppState>,
    query: String,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<TaxonSearchResult>> {
    mapping::search_photo_taxa(
        &state.database,
        &query,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn suggest_photo_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSuggestion>> {
    mapping::suggest_photo_taxa(&state.database, &query, limit.unwrap_or(10)).map_err(error)
}

#[tauri::command]
pub fn get_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<Option<PhotoTaxonMapping>> {
    mapping::get_photo_mapping(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn clear_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoTaxonMapping> {
    mapping::clear_photo_mapping(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn set_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
    taxon_id: i64,
) -> CommandResult<PhotoTaxonMapping> {
    mapping::set_photo_mapping(&state.database, photo_id, taxon_id).map_err(error)
}

#[tauri::command]
pub fn remap_photo(state: State<'_, AppState>, photo_id: i64) -> CommandResult<PhotoTaxonMatch> {
    mapping::remap_photo(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn list_taxon_photos(
    state: State<'_, AppState>,
    taxon_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<Photo>> {
    mapping::list_taxon_photos(
        &state.database,
        taxon_id,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
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
pub fn search_photos_by_mapping_status(
    state: State<'_, AppState>,
    status: PhotoMappingListStatus,
    query: String,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoMappingListItem>> {
    mapping::search_photos_by_mapping_status(
        &state.database,
        status,
        &query,
        cursor.as_deref(),
        limit.unwrap_or(50),
    )
    .map_err(error)
}

#[tauri::command]
pub fn get_map_settings(state: State<'_, AppState>) -> CommandResult<MapSettings> {
    map::get_map_settings(&state.database).map_err(error)
}

#[tauri::command]
pub fn set_map_settings(
    state: State<'_, AppState>,
    settings: MapSettings,
) -> CommandResult<MapSettings> {
    map::set_map_settings(&state.database, settings).map_err(error)
}

#[tauri::command]
pub fn list_map_photos(
    state: State<'_, AppState>,
    bounds: Option<MapBounds>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<MapPhoto>> {
    map::list_map_photos(
        &state.database,
        bounds,
        cursor.as_deref(),
        limit.unwrap_or(500),
    )
    .map_err(error)
}

#[tauri::command]
pub fn get_operations_status(state: State<'_, AppState>) -> OperationsStatus {
    state.operations.status()
}

fn error(error: impl ToString) -> String {
    error.to_string()
}
