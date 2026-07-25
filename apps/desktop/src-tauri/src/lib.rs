mod commands;
mod file_manager;
mod media;
mod paths;
mod state;
mod updater;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let state = AppState::new(paths::data_dir(app.handle())?)?;
            state::set_global(state.clone())
                .map_err(|_| std::io::Error::other("application state already initialized"))?;
            app.manage(state);
            app.manage(updater::PendingAppUpdate::default());
            Ok(())
        })
        .register_uri_scheme_protocol("phytoindex", |_context, request| media::handle(request))
        .invoke_handler(tauri::generate_handler![
            commands::get_photo_library,
            commands::get_photo_library_count,
            commands::open_photo_library,
            commands::browse_photo_directory,
            commands::get_photo_directory_counts,
            commands::refresh_photo_directory,
            commands::start_photo_mapping,
            commands::parse_photo_filename,
            commands::normalize_taxonomy_name,
            commands::get_naming_hook_settings,
            commands::get_naming_hook_templates,
            commands::set_naming_hook,
            commands::test_naming_hook,
            commands::get_naming_hook_test_cases,
            commands::set_naming_hook_test_cases,
            commands::run_naming_hook_tests,
            commands::get_photo_name_match_settings,
            commands::set_photo_name_match_settings,
            commands::get_photo_filename_format_settings,
            commands::set_photo_filename_format_settings,
            commands::format_photo_filename,
            commands::rename_photo,
            commands::rename_photo_from_taxon,
            commands::rename_photos_from_taxa,
            commands::rename_photos_in_directory_from_taxa,
            commands::list_photo_operations,
            commands::get_photo_operation,
            commands::revert_photo_operation,
            commands::export_photo_operation_csv,
            commands::export_all_photo_operations_csv,
            commands::get_photo,
            commands::search_photos,
            commands::search_photos_by_filename,
            commands::get_photo_metadata,
            commands::get_photo_availability,
            commands::reveal_photo_in_file_manager,
            commands::search_taxa,
            commands::suggest_taxa,
            commands::get_taxon_detail_node,
            commands::list_taxon_children,
            commands::delete_taxon_name,
            commands::update_taxon,
            commands::promote_taxon_name,
            commands::delete_taxon,
            commands::execute_custom_taxonomy_sql,
            commands::preview_taxonomy_rows,
            commands::apply_taxonomy_rows,
            commands::parse_taxonomy_input_csv,
            commands::get_taxonomy_formatted_update_template,
            commands::export_taxonomy_log,
            commands::get_taxonomy_name_separator,
            commands::set_taxonomy_name_separator,
            commands::list_taxonomy_operations,
            commands::get_taxonomy_operation,
            commands::revert_taxonomy_operation,
            commands::export_taxonomy_operation_csv,
            commands::export_all_taxonomy_operations_csv,
            commands::get_taxonomy_base_metadata,
            commands::replace_taxonomy_base_database,
            commands::get_mapping_metadata,
            commands::search_photo_taxa,
            commands::suggest_photo_taxa,
            commands::get_photo_mapping,
            commands::clear_photo_mapping,
            commands::set_photo_mapping,
            commands::remap_photo,
            commands::list_taxon_photos,
            commands::get_photo_taxon_match,
            commands::select_photo_taxon,
            commands::get_photo_taxon_node,
            commands::browse_photo_taxon,
            commands::list_photos_by_mapping_status,
            commands::search_photos_by_mapping_status,
            commands::get_map_settings,
            commands::set_map_settings,
            commands::list_map_photos,
            commands::get_app_version,
            commands::check_app_update,
            commands::install_app_update,
            commands::get_operations_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vividarium");
}
