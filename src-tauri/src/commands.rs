use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    db,
    models::{AppView, SettingsPayload, Snapshot},
    service, AppState,
};

#[tauri::command]
pub fn bootstrap(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    let is_scanning = *state.is_scanning.lock().map_err(|err| err.to_string())?;
    service::snapshot(&app, is_scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: SettingsPayload,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    if settings.api_key.trim().is_empty() {
        let conn = db::connect(&app).map_err(|err| err.to_string())?;
        db::write_settings(&conn, &settings).map_err(|err| err.to_string())?;
        db::write_active_view(&conn, &AppView::Settings).map_err(|err| err.to_string())?;
        service::clear_last_error(&app);
        {
            let mut scanning = state.is_scanning.lock().map_err(|err| err.to_string())?;
            *scanning = false;
        }
        if settings.auto_start {
            app.autolaunch().enable().map_err(|err| err.to_string())?;
        } else {
            app.autolaunch().disable().map_err(|err| err.to_string())?;
        }
        return service::snapshot(&app, false).map_err(|err| err.to_string());
    }

    service::validate_api_key_for_settings(&app, &settings.api_key)
        .await
        .map_err(|err| err.to_string())?;

    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::write_settings(&conn, &settings).map_err(|err| err.to_string())?;
    db::write_active_view(&conn, &AppView::Reading).map_err(|err| err.to_string())?;
    if settings.auto_start {
        app.autolaunch().enable().map_err(|err| err.to_string())?;
    } else {
        app.autolaunch().disable().map_err(|err| err.to_string())?;
    }

    {
        let mut scanning = state.is_scanning.lock().map_err(|err| err.to_string())?;
        *scanning = true;
    }

    service::ensure_scheduler(&app);
    service::trigger_fetch_now(&app, None);
    service::snapshot(&app, true).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_article(app: AppHandle, article_id: i64) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::mark_article_opened(&conn, article_id).map_err(|err| err.to_string())?;
    let source_id = db::article_source_id(&conn, article_id).map_err(|err| err.to_string())?;
    db::log_user_event(
        &conn,
        "open-article",
        Some(article_id),
        source_id.as_deref(),
        Some(r#"{"origin":"main"}"#),
    )
    .map_err(|err| err.to_string())?;
    let settings = db::read_settings(&conn).map_err(|err| err.to_string())?;
    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)
        .map_err(|err| err.to_string())?;
    service::sync_windows(&app, false).map_err(|err| err.to_string())?;
    service::snapshot(&app, false).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn toggle_favorite(app: AppHandle, article_id: i64) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    let is_favorite = db::toggle_favorite(&conn, article_id).map_err(|err| err.to_string())?;
    let source_id = db::article_source_id(&conn, article_id).map_err(|err| err.to_string())?;
    db::log_user_event(
        &conn,
        if is_favorite {
            "favorite-added"
        } else {
            "favorite-removed"
        },
        Some(article_id),
        source_id.as_deref(),
        None,
    )
    .map_err(|err| err.to_string())?;
    let settings = db::read_settings(&conn).map_err(|err| err.to_string())?;
    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)
        .map_err(|err| err.to_string())?;
    service::snapshot(&app, false).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn pet_double_click(app: AppHandle) -> Result<(), String> {
    service::handle_pet_double_click(&app).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn bubble_action(app: AppHandle, action: String) -> Result<Snapshot, String> {
    service::handle_bubble_action(&app, &action).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn set_active_view(app: AppHandle, view: AppView) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::write_active_view(&conn, &view).map_err(|err| err.to_string())?;
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    service::snapshot(&app, false).map_err(|err| err.to_string())
}
