use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, State, ipc::Channel};
use vividarium_core::general::{GeneralSettings, WorkspaceState};
use vividarium_core::mapping::{
    PhotoMappingListItem, PhotoMappingListStatus, PhotoMappingSummary, PhotoNameMatchSettings,
    PhotoTaxonCandidate, PhotoTaxonItem, PhotoTaxonNode,
};
use vividarium_core::models::{
    DatabaseLocations, DirectoryEntryCounts, MappingMetadata, OperationState, OperationsStatus,
    Photo, PhotoDirectory, PhotoDirectoryItem, PhotoLibrary, PhotoLibraryRegistration,
    PhotoMetadata, PhotoPage,
};
use vividarium_core::naming::{
    NamingHookKind, NamingHookSettings, NamingHookTemplates, NamingHookTestCase,
    NamingHookTestCases, NamingHookTestReport, NamingHookTestResult, ParsedPhotoFilename,
    TaxonomicNameInfo,
};
use vividarium_core::operations::{OperationAuditRow, OperationPage, OperationSummary};
use vividarium_core::photos::{PhotoFilenameFormatSettings, PhotoRenameOperationResult};
use vividarium_core::taxonomy::{
    AddSqlInputRequest, AddSqlInputResult, CustomSqlExecutionResult,
    CustomTaxonomySqlExportRequest, CustomTaxonomySqlRequest, DeleteTaxonNameInput,
    DirectImportDatabase, PersistentSqlInput, PromoteTaxonNameInput, RemoveSqlInputRequest,
    RemoveSqlInputResult, SaveTaxonNameGroupInput, SqlExportResult, SqlSourceSchema, TaxonChild,
    TaxonDetail, TaxonInputRow, TaxonRowOutcome, TaxonSearchResult, TaxonSuggestion, TaxonUpdateInput,
    TaxonomyImportMetadata, TaxonomyOperationResult, TaxonomyPage, TaxonomyPreviewResult,
    ValidateSqlImportRequest,
};
use vividarium_core::{
    map::{self, MapBounds, MapPhoto, MapSettings},
    mapping, naming, photos, taxonomy,
};

use crate::state::AppState;
use crate::updater::{AppUpdateEvent, AppUpdateInfo, PendingAppUpdate};

type CommandResult<T> = Result<T, String>;

#[derive(Debug, Serialize)]
pub struct PhotoLibraryWorkspace {
    pub library_uuid: String,
    pub display_name: String,
    pub root_path: String,
    pub db_path: String,
    pub last_opened_at: String,
    pub active: bool,
    pub root_available: bool,
    pub database_available: bool,
}

#[derive(Debug, Serialize)]
pub struct FormattedUpdatePreviewResult {
    pub preview_id: String,
    pub delimiter: String,
    pub encoding: String,
    pub rows: Vec<TaxonRowOutcome>,
}

impl FormattedUpdatePreviewResult {
    fn new(preview_id: String, preview: TaxonomyPreviewResult) -> Self {
        Self {
            preview_id,
            delimiter: preview.delimiter,
            encoding: preview.encoding,
            rows: preview.rows,
        }
    }
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub fn get_general_settings(state: State<'_, AppState>) -> CommandResult<GeneralSettings> {
    vividarium_core::general::get_general_settings(&state.database).map_err(error)
}

#[tauri::command]
pub fn update_general_settings(
    state: State<'_, AppState>,
    settings: GeneralSettings,
) -> CommandResult<GeneralSettings> {
    vividarium_core::general::update_general_settings(&state.database, &settings).map_err(error)
}

#[tauri::command]
pub fn get_workspace_state(state: State<'_, AppState>) -> CommandResult<WorkspaceState> {
    vividarium_core::general::get_workspace_state(&state.database).map_err(error)
}

#[tauri::command]
pub fn save_workspace_state(
    state: State<'_, AppState>,
    workspace_state: WorkspaceState,
) -> CommandResult<()> {
    vividarium_core::general::save_workspace_state(&state.database, &workspace_state).map_err(error)
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
pub fn open_photo_library(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> CommandResult<PhotoLibrary> {
    let library = photos::open_library(&state.database, &root).map_err(error)?;
    schedule_taxonomy_sync(app, &state);
    Ok(library)
}

#[tauri::command]
pub fn get_database_locations(state: State<'_, AppState>) -> CommandResult<DatabaseLocations> {
    state.database.locations().map_err(error)
}

#[tauri::command]
pub fn list_photo_libraries(
    state: State<'_, AppState>,
) -> CommandResult<Vec<PhotoLibraryWorkspace>> {
    let active_library_uuid = state
        .database
        .locations()
        .map_err(error)?
        .active_photo_library_uuid;
    state
        .database
        .list_photo_libraries()
        .map_err(error)
        .map(|libraries| {
            libraries
                .into_iter()
                .map(|library| PhotoLibraryWorkspace {
                    active: active_library_uuid.as_deref() == Some(&library.library_uuid),
                    root_available: Path::new(&library.root_path).is_dir(),
                    database_available: Path::new(&library.db_path).is_file(),
                    library_uuid: library.library_uuid,
                    display_name: library.display_name,
                    root_path: library.root_path,
                    db_path: library.db_path,
                    last_opened_at: library.last_opened_at,
                })
                .collect()
        })
}

#[tauri::command]
pub fn register_photo_library(
    app: AppHandle,
    state: State<'_, AppState>,
    root_path: String,
    database_path: String,
    display_name: Option<String>,
) -> CommandResult<PhotoLibraryRegistration> {
    let library = state
        .database
        .register_photo_library(
            Path::new(&root_path),
            Path::new(&database_path),
            display_name.as_deref(),
        )
        .map_err(error)?;
    schedule_taxonomy_sync(app, &state);
    Ok(library)
}

#[tauri::command]
pub fn switch_photo_library(
    app: AppHandle,
    state: State<'_, AppState>,
    library_uuid: String,
) -> CommandResult<PhotoLibraryRegistration> {
    let library = state
        .database
        .switch_photo_library(&library_uuid)
        .map_err(error)?;
    schedule_taxonomy_sync(app, &state);
    Ok(library)
}

#[tauri::command]
pub fn rebind_photo_library_root(
    state: State<'_, AppState>,
    library_uuid: String,
    root_path: String,
) -> CommandResult<PhotoLibraryRegistration> {
    state
        .database
        .rebind_photo_library_root(&library_uuid, Path::new(&root_path))
        .map_err(error)
}

#[tauri::command]
pub fn rebind_photo_library_database(
    app: AppHandle,
    state: State<'_, AppState>,
    library_uuid: String,
    database_path: String,
) -> CommandResult<PhotoLibraryRegistration> {
    ensure_database_relocation_allowed(&state)?;
    let library = state
        .database
        .rebind_photo_library_database(&library_uuid, Path::new(&database_path))
        .map_err(error)?;
    schedule_taxonomy_sync(app, &state);
    Ok(library)
}

#[tauri::command]
pub fn relocate_photo_library_database(
    state: State<'_, AppState>,
    library_uuid: String,
    database_path: String,
) -> CommandResult<PhotoLibraryRegistration> {
    ensure_database_relocation_allowed(&state)?;
    state
        .database
        .relocate_photo_library_database(&library_uuid, Path::new(&database_path))
        .map_err(error)
}

#[tauri::command]
pub fn remove_photo_library(state: State<'_, AppState>, library_uuid: String) -> CommandResult<()> {
    state
        .database
        .remove_photo_library(&library_uuid)
        .map_err(error)
}

#[tauri::command]
pub fn rename_photo_library(
    state: State<'_, AppState>,
    library_uuid: String,
    display_name: String,
) -> CommandResult<PhotoLibraryRegistration> {
    state
        .database
        .rename_photo_library(&library_uuid, &display_name)
        .map_err(error)
}

#[tauri::command]
pub fn open_path_in_file_manager(path: String) -> CommandResult<()> {
    let path = Path::new(&path);
    if !path.is_absolute() {
        return Err("storage path must be absolute".into());
    }
    if path.is_dir() {
        crate::file_manager::open_directory(path)
    } else if path.is_file() {
        crate::file_manager::reveal(path)
    } else {
        Err(format!("storage path is unavailable: {}", path.display()))
    }
}

#[tauri::command]
pub fn relocate_taxonomy_database(
    state: State<'_, AppState>,
    database_path: String,
) -> CommandResult<DatabaseLocations> {
    ensure_database_relocation_allowed(&state)?;
    state
        .database
        .relocate_taxonomy_database(Path::new(&database_path))
        .map_err(error)
}

#[tauri::command]
pub fn open_taxonomy_database(
    app: AppHandle,
    state: State<'_, AppState>,
    database_path: String,
) -> CommandResult<DatabaseLocations> {
    ensure_database_relocation_allowed(&state)?;
    let locations = state
        .database
        .open_taxonomy_database(Path::new(&database_path))
        .map_err(error)?;
    schedule_taxonomy_sync(app, &state);
    Ok(locations)
}

#[tauri::command]
pub fn set_default_taxonomy_database_directory(
    state: State<'_, AppState>,
    directory: String,
) -> CommandResult<DatabaseLocations> {
    state
        .database
        .set_default_taxonomy_directory(Path::new(&directory))
        .map_err(error)
}

#[tauri::command]
pub fn set_default_photo_library_database_directory(
    state: State<'_, AppState>,
    directory: String,
) -> CommandResult<DatabaseLocations> {
    state
        .database
        .set_default_photo_library_directory(Path::new(&directory))
        .map_err(error)
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
            taxonomy::synchronize_pending_photo_libraries(&database).map_err(error)?;
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
            taxonomy::synchronize_pending_photo_libraries(&database).map_err(error)?;
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
pub fn run_naming_hook_tests(
    kind: NamingHookKind,
    script: String,
    cases: Vec<NamingHookTestCase>,
) -> CommandResult<NamingHookTestReport> {
    naming::run_naming_hook_tests(kind, &script, &cases).map_err(error)
}

#[tauri::command]
pub fn save_naming_hook(
    state: State<'_, AppState>,
    kind: NamingHookKind,
    script: String,
    cases: Vec<NamingHookTestCase>,
) -> CommandResult<()> {
    naming::save_naming_hook(&state.database, kind, &script, &cases).map_err(error)
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
pub fn rename_photo_directory(
    state: State<'_, AppState>,
    directory_id: i64,
    new_name: String,
) -> CommandResult<PhotoDirectory> {
    photos::rename_directory(&state.database, directory_id, &new_name).map_err(error)
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
pub async fn list_photo_operations(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<OperationPage<OperationSummary>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        photos::list_operations(&database, cursor.as_deref(), limit.unwrap_or(50)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn list_photo_operation_audit(
    state: State<'_, AppState>,
    operation_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<OperationPage<OperationAuditRow>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        photos::list_operation_audit(
            &database,
            operation_id,
            cursor.as_deref(),
            limit.unwrap_or(50),
        )
        .map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn rollback_photo_operation(
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        photos::rollback_operation(&database, operation_id).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_photo_operation_audit(
    state: State<'_, AppState>,
    operation_id: i64,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        photos::write_operation_audit(&database, operation_id, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_photo_operations_audit(
    state: State<'_, AppState>,
    operation_ids: Vec<i64>,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        photos::write_operations_audit(&database, &operation_ids, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_all_photo_operation_audit(
    state: State<'_, AppState>,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        photos::write_all_operation_audit(&database, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn get_photo(state: State<'_, AppState>, photo_id: i64) -> CommandResult<Photo> {
    photos::get_photo(&state.database, photo_id)
        .map_err(error)?
        .ok_or_else(|| format!("photo {photo_id} not found"))
}

#[tauri::command]
pub async fn search_photos(
    state: State<'_, AppState>,
    query: String,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<Photo>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        photos::search_photos(&database, &query, cursor.as_deref(), limit.unwrap_or(50))
            .map_err(error)
    })
    .await
    .map_err(error)?
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
pub fn open_photo_directory_in_file_manager(
    state: State<'_, AppState>,
    directory_id: i64,
) -> CommandResult<()> {
    let path = photos::photo_directory_path(&state.database, directory_id).map_err(error)?;
    crate::file_manager::open_directory(&path)
}

#[tauri::command]
pub fn get_photo_metadata(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoMetadata> {
    photos::get_photo_metadata(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub async fn search_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSearchResult>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::search_taxa(&database, &query, limit.unwrap_or(50)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn suggest_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSuggestion>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::suggest_taxa(&database, &query, limit.unwrap_or(10)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn get_taxon_detail(
    state: State<'_, AppState>,
    taxon_id: i64,
) -> CommandResult<TaxonDetail> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::get_taxon_detail(&database, taxon_id)
            .map_err(error)?
            .ok_or_else(|| format!("taxon {taxon_id} not found"))
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn list_taxon_children(
    state: State<'_, AppState>,
    taxon_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<TaxonomyPage<TaxonChild>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::list_taxon_children(&database, taxon_id, cursor.as_deref(), limit.unwrap_or(50))
            .map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn delete_taxon_name(
    app: AppHandle,
    state: State<'_, AppState>,
    input: DeleteTaxonNameInput,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::delete_taxon_name(&database, input).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn update_taxon(
    app: AppHandle,
    state: State<'_, AppState>,
    input: TaxonUpdateInput,
) -> CommandResult<TaxonomyOperationResult> {
    let database = state.database.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        taxonomy::update_taxon(&database, input).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(result)
}

#[tauri::command]
pub async fn promote_taxon_name(
    app: AppHandle,
    state: State<'_, AppState>,
    input: PromoteTaxonNameInput,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::promote_taxon_name(&database, input).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn save_taxon_name_group(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SaveTaxonNameGroupInput,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::save_taxon_name_group(&database, input).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn delete_taxon(
    app: AppHandle,
    state: State<'_, AppState>,
    taxon_id: i64,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::delete_taxon(&database, taxon_id).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn execute_custom_taxonomy_sql(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CustomTaxonomySqlRequest,
) -> CommandResult<CustomSqlExecutionResult> {
    let database = state.database.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        taxonomy::execute_custom_taxonomy_sql(&database, &request).map_err(error)
    })
    .await
    .map_err(error)??;
    if result.changeset_size > 0 {
        schedule_taxonomy_sync(app, &state);
    }
    Ok(result)
}

#[tauri::command]
pub fn get_custom_taxonomy_sql(state: State<'_, AppState>) -> CommandResult<String> {
    taxonomy::get_custom_taxonomy_sql(&state.database).map_err(error)
}

#[tauri::command]
pub fn list_custom_sql_inputs(
    state: State<'_, AppState>,
) -> CommandResult<Vec<PersistentSqlInput>> {
    taxonomy::list_custom_sql_inputs(&state.database).map_err(error)
}

#[tauri::command]
pub fn list_custom_sql_database_schemas(
    state: State<'_, AppState>,
) -> CommandResult<Vec<SqlSourceSchema>> {
    taxonomy::list_custom_sql_database_schemas(&state.database).map_err(error)
}

#[tauri::command]
pub async fn add_custom_sql_input(
    state: State<'_, AppState>,
    request: AddSqlInputRequest,
) -> CommandResult<AddSqlInputResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::add_custom_sql_input(&database, &request).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn remove_custom_sql_input(
    state: State<'_, AppState>,
    request: RemoveSqlInputRequest,
) -> CommandResult<RemoveSqlInputResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::remove_custom_sql_input(&database, &request).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_custom_taxonomy_query(
    state: State<'_, AppState>,
    request: CustomTaxonomySqlExportRequest,
) -> CommandResult<SqlExportResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::export_custom_taxonomy_query(&database, &request).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn preview_taxonomy_rows(
    state: State<'_, AppState>,
    rows: Vec<TaxonInputRow>,
) -> CommandResult<FormattedUpdatePreviewResult> {
    state.clear_formatted_update_preview().map_err(error)?;
    let database = state.database.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        taxonomy::prepare_rows(&database, &rows).map_err(error)
    })
    .await
    .map_err(error)??;
    let (preview_id, preview) = state
        .replace_formatted_update_preview(prepared)
        .map_err(error)?;
    Ok(FormattedUpdatePreviewResult::new(preview_id, preview))
}

#[tauri::command]
pub async fn apply_taxonomy_rows(
    app: AppHandle,
    state: State<'_, AppState>,
    preview_id: String,
) -> CommandResult<TaxonomyOperationResult> {
    let prepared = state
        .take_formatted_update_preview(&preview_id)
        .map_err(error)?;
    let database = state.database.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        taxonomy::apply_prepared_rows(&database, prepared).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(result)
}

#[tauri::command]
pub async fn parse_taxonomy_input_csv(
    state: State<'_, AppState>,
    input: String,
) -> CommandResult<Vec<TaxonInputRow>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::parse_taxonomy_input_csv(&database, &input).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn get_taxonomy_formatted_update_template(state: State<'_, AppState>) -> CommandResult<String> {
    taxonomy::taxonomy_formatted_update_template(&state.database).map_err(error)
}

#[tauri::command]
pub fn export_taxonomy_log(
    state: State<'_, AppState>,
    rows: Vec<TaxonRowOutcome>,
) -> CommandResult<String> {
    taxonomy::taxonomy_log_csv(&state.database, &rows).map_err(error)
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
pub async fn list_taxonomy_operations(
    state: State<'_, AppState>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<OperationPage<OperationSummary>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::list_operations(&database, cursor.as_deref(), limit.unwrap_or(50)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn list_taxonomy_operation_audit(
    state: State<'_, AppState>,
    operation_id: i64,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<OperationPage<OperationAuditRow>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::list_operation_audit(
            &database,
            operation_id,
            cursor.as_deref(),
            limit.unwrap_or(50),
        )
        .map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn rollback_taxonomy_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    operation_id: i64,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::rollback_operation(&database, operation_id).map_err(error)
    })
    .await
    .map_err(error)??;
    schedule_taxonomy_sync(app, &state);
    Ok(())
}

#[tauri::command]
pub async fn export_taxonomy_operation_audit(
    state: State<'_, AppState>,
    operation_id: i64,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        taxonomy::write_operation_audit(&database, operation_id, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_taxonomy_operations_audit(
    state: State<'_, AppState>,
    operation_ids: Vec<i64>,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        taxonomy::write_operations_audit(&database, &operation_ids, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_all_taxonomy_operation_audit(
    state: State<'_, AppState>,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut writer = audit_writer(&destination_path)?;
        taxonomy::write_all_operation_audit(&database, &mut writer).map_err(error)?;
        writer.flush().map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_taxonomy_operation_input(
    state: State<'_, AppState>,
    operation_id: i64,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let csv = taxonomy::export_operation_input(&database, operation_id).map_err(error)?;
        write_csv_export(&destination_path, &csv)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_taxonomy_operations_input(
    state: State<'_, AppState>,
    operation_ids: Vec<i64>,
    destination_path: String,
) -> CommandResult<()> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let csv = taxonomy::export_operations_input(&database, &operation_ids).map_err(error)?;
        write_csv_export(&destination_path, &csv)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn export_all_replayable_taxonomy_inputs(
    state: State<'_, AppState>,
) -> CommandResult<String> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::export_all_replayable_inputs(&database).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn get_taxonomy_import_metadata(
    state: State<'_, AppState>,
) -> CommandResult<Option<TaxonomyImportMetadata>> {
    taxonomy::get_taxonomy_import_metadata(&state.database).map_err(error)
}

#[tauri::command]
pub async fn inspect_direct_import_database(
    state: State<'_, AppState>,
    source_path: String,
) -> CommandResult<DirectImportDatabase> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::inspect_direct_import_database(&database, Path::new(&source_path)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn get_sql_import_sql(state: State<'_, AppState>) -> CommandResult<String> {
    taxonomy::get_sql_import_sql(&state.database).map_err(error)
}

#[tauri::command]
pub fn list_sql_import_inputs(
    state: State<'_, AppState>,
) -> CommandResult<Vec<PersistentSqlInput>> {
    taxonomy::list_sql_import_inputs(&state.database).map_err(error)
}

#[tauri::command]
pub fn list_sql_import_database_schemas(
    state: State<'_, AppState>,
) -> CommandResult<Vec<SqlSourceSchema>> {
    taxonomy::list_sql_import_database_schemas(&state.database).map_err(error)
}

#[tauri::command]
pub fn list_sql_import_staging_schemas(
    state: State<'_, AppState>,
) -> CommandResult<Vec<SqlSourceSchema>> {
    taxonomy::list_sql_import_staging_schemas(&state.database).map_err(error)
}

#[tauri::command]
pub async fn add_sql_import_input(
    state: State<'_, AppState>,
    request: AddSqlInputRequest,
) -> CommandResult<AddSqlInputResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::add_sql_import_input(&database, &request).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn remove_sql_import_input(
    state: State<'_, AppState>,
    request: RemoveSqlInputRequest,
) -> CommandResult<RemoveSqlInputResult> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        taxonomy::remove_sql_import_input(&database, &request).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn start_sql_import_validation(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ValidateSqlImportRequest,
) -> CommandResult<OperationState> {
    let database = state.database.clone();
    state.operations.start_with_progress(
        app,
        "sql_import",
        "validate_sql_import",
        move |progress| match taxonomy::validate_sql_import_with_progress(
            &database, &request, progress,
        ) {
            Ok(result) => serde_json::to_value(result).map_err(error),
            Err(failure) => {
                progress(vividarium_core::OperationProgress {
                    stage: "operational_failure".into(),
                    current: None,
                    total: None,
                    statement_index: None,
                    statement_total: None,
                });
                Err(error(failure))
            }
        },
    )
}

#[tauri::command]
pub fn apply_sql_import(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CommandResult<OperationState> {
    let database = state.database.clone();
    let background_state = state.inner().clone();
    let sync_app = app.clone();
    state
        .operations
        .start(app, "sql_import", "apply_sql_import", move |progress| {
            progress(0, None, "Validating SQL import candidate");
            let result = taxonomy::apply_sql_import(&database).map_err(error)?;
            progress(1, Some(1), "Taxonomy SQL import applied");
            schedule_taxonomy_sync(sync_app, &background_state);
            serde_json::to_value(result).map_err(error)
        })
}

#[tauri::command]
pub fn apply_direct_import(
    app: AppHandle,
    state: State<'_, AppState>,
    source_path: String,
) -> CommandResult<OperationState> {
    let database = state.database.clone();
    let background_state = state.inner().clone();
    let sync_app = app.clone();
    state.operations.start(
        app,
        "direct_import",
        "apply_direct_import",
        move |progress| {
            progress(0, None, "Validating direct import database");
            let result =
                taxonomy::apply_direct_import(&database, Path::new(&source_path)).map_err(error)?;
            progress(1, Some(1), "Direct import applied");
            schedule_taxonomy_sync(sync_app, &background_state);
            serde_json::to_value(result).map_err(error)
        },
    )
}

fn schedule_taxonomy_sync(app: AppHandle, state: &AppState) {
    if !state.taxonomy_sync.request() {
        return;
    }
    let state = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            if !state.taxonomy_sync.take_request() {
                if state.taxonomy_sync.release_or_continue() {
                    continue;
                }
                break;
            }
            while state
                .operations
                .status()
                .values()
                .any(|operation| operation.running)
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let database = state.database.clone();
            let operation =
                state
                    .operations
                    .start(app.clone(), "mapping", "taxonomy_sync", move |progress| {
                        progress(0, None, "Synchronizing taxonomy changes");
                        let sync = taxonomy::synchronize_pending_photo_libraries(&database)
                            .map_err(error)?;
                        let mapping = mapping::process_pending_photo_matches(&database, progress)
                            .map_err(error)?;
                        Ok(json!({ "sync": sync, "mapping": mapping }))
                    });
            let task_id = match operation {
                Ok(operation) => operation.task_id,
                Err(_) => {
                    state.taxonomy_sync.request();
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
            };
            while state.operations.status().values().any(|operation| {
                operation.running && operation.task_id.as_deref() == task_id.as_deref()
            }) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    });
}

fn audit_writer(destination_path: &str) -> CommandResult<BufWriter<File>> {
    let destination = Path::new(destination_path);
    if !destination.is_absolute() {
        return Err("CSV export destination must be an absolute path".into());
    }
    File::create(destination).map(BufWriter::new).map_err(error)
}

fn write_csv_export(destination_path: &str, contents: &str) -> CommandResult<()> {
    let mut writer = audit_writer(destination_path)?;
    writer.write_all(contents.as_bytes()).map_err(error)?;
    writer.flush().map_err(error)
}

fn ensure_database_relocation_allowed(state: &AppState) -> CommandResult<()> {
    let running = state
        .operations
        .status()
        .into_values()
        .filter(|operation| operation.running)
        .filter_map(|operation| operation.operation)
        .collect::<Vec<_>>();
    if running.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "database relocation is blocked by running operations: {}",
            running.join(", ")
        ))
    }
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
pub async fn suggest_photo_taxa(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> CommandResult<Vec<TaxonSuggestion>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        mapping::suggest_photo_taxa(&database, &query, limit.unwrap_or(10)).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub fn get_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoMappingSummary> {
    mapping::get_photo_mapping(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn get_photo_mapping_candidates(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<Vec<PhotoTaxonCandidate>> {
    mapping::get_photo_mapping_candidates(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn clear_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoMappingSummary> {
    mapping::clear_photo_mapping(&state.database, photo_id).map_err(error)
}

#[tauri::command]
pub fn set_photo_mapping(
    state: State<'_, AppState>,
    photo_id: i64,
    taxon_id: i64,
) -> CommandResult<PhotoMappingSummary> {
    mapping::set_photo_mapping(&state.database, photo_id, taxon_id).map_err(error)
}

#[tauri::command]
pub fn remap_photo(
    state: State<'_, AppState>,
    photo_id: i64,
) -> CommandResult<PhotoMappingSummary> {
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
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<PhotoTaxonItem>> {
    mapping::browse_photo_taxon(
        &state.database,
        taxon_id,
        show_empty.unwrap_or(false),
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
