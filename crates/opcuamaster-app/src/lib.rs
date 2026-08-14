mod commands;
mod state;
pub mod update;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .manage(update::UpdateState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::create_connection,
            commands::connect,
            commands::disconnect,
            commands::delete_connection,
            commands::list_connections,
            commands::discover_endpoints,
            commands::browse_root,
            commands::browse_node,
            commands::collect_variables,
            commands::read_attributes,
            commands::write_value,
            commands::add_monitored_nodes,
            commands::add_variables_under_node,
            commands::remove_monitored_nodes,
            commands::get_monitored_nodes_since,
            commands::get_polling_nodes,
            commands::read_history,
            commands::subscribe_events,
            commands::unsubscribe_events,
            commands::clear_events,
            commands::get_events,
            commands::read_method_arguments,
            commands::call_method,
            commands::list_certificates,
            commands::move_certificate,
            commands::delete_certificate,
            commands::create_group,
            commands::delete_group,
            commands::add_to_group,
            commands::list_groups,
            commands::save_project,
            commands::load_project,
            commands::get_communication_logs,
            commands::clear_communication_logs,
            commands::export_communication_logs,
            update::check_for_update,
            update::install_update,
            update::skip_update,
            update::schedule_update_on_next_launch,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = update::install_pending_update(app_handle).await {
                    log::warn!("automatic update on launch failed: {error}");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
