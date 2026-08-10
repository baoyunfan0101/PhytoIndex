use tauri::State;
use vividarium_core::map::{self, MapBounds, MapPhoto, MapSettings};
use vividarium_core::models::PhotoPage;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

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
pub async fn get_map_photo_bounds(state: State<'_, AppState>) -> CommandResult<Option<MapBounds>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        map::get_map_photo_bounds(&database).map_err(error)
    })
    .await
    .map_err(error)?
}

#[tauri::command]
pub async fn list_map_photos(
    state: State<'_, AppState>,
    bounds: Option<MapBounds>,
    cursor: Option<String>,
    limit: Option<usize>,
) -> CommandResult<PhotoPage<MapPhoto>> {
    let database = state.database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        map::list_map_photos(&database, bounds, cursor.as_deref(), limit.unwrap_or(500))
            .map_err(error)
    })
    .await
    .map_err(error)?
}

fn error(error: impl ToString) -> String {
    error.to_string()
}
