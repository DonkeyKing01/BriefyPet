use tauri::{api::process, AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    db,
    models::{
        AppView, Discipline, HistoryItem, OverlaySnapshot, ResourceType, RssSource,
        SettingsPayload, Snapshot, SourceKind,
    },
    policy, service, AppState,
};

#[tauri::command]
pub fn bootstrap(app: AppHandle, state: State<AppState>) -> Result<Snapshot, String> {
    service::reconcile_fetch_runtime(&app).map_err(|err| err.to_string())?;
    let is_scanning = *state.is_scanning.lock().map_err(|err| err.to_string())?;
    service::snapshot(&app, is_scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn bootstrap_overlay(
    app: AppHandle,
    state: State<AppState>,
) -> Result<OverlaySnapshot, String> {
    service::reconcile_fetch_runtime(&app).map_err(|err| err.to_string())?;
    let is_scanning = *state.is_scanning.lock().map_err(|err| err.to_string())?;
    service::snapshot_overlay(&app, is_scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: SettingsPayload,
    state: State<'_, AppState>,
) -> Result<Snapshot, String> {
    if service::requires_configuration(&settings, None) {
        let conn = db::connect(&app).map_err(|err| err.to_string())?;
        db::write_settings(&conn, &settings).map_err(|err| err.to_string())?;
        db::write_active_view(&conn, &AppView::Settings).map_err(|err| err.to_string())?;
        if settings.api_key.trim().is_empty() {
            db::write_api_key_valid(&conn, false).map_err(|err| err.to_string())?;
        }
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
        service::sync_windows(&app, false).map_err(|err| err.to_string())?;
        return service::publish_snapshot(&app, false).map_err(|err| err.to_string());
    }

    service::validate_api_key_for_settings(&app, &settings)
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

    service::reconcile_fetch_runtime(&app).map_err(|err| err.to_string())?;
    let scanning = service::current_scanning(&app);
    service::sync_windows(&app, scanning).map_err(|err| err.to_string())?;
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_article(app: AppHandle, article_id: i64) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::mark_article_opened(&conn, article_id).map_err(|err| err.to_string())?;
    let _ = db::mark_push_article_pushed(&app, article_id).map_err(|err| err.to_string())?;
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
    let scanning = service::current_scanning(&app);
    service::sync_windows(&app, scanning).map_err(|err| err.to_string())?;
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
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
    let scanning = service::current_scanning(&app);
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn pet_double_click(app: AppHandle) -> Result<(), String> {
    service::handle_pet_double_click(&app).map_err(|err| err.to_string())?;
    let scanning = service::current_scanning(&app);
    service::sync_windows(&app, scanning).map_err(|err| err.to_string())?;
    let _ = service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn bubble_action(app: AppHandle, action: String) -> Result<Snapshot, String> {
    service::handle_bubble_action(&app, &action).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_help_window(app: AppHandle) -> Result<(), String> {
    service::show_help_window(&app).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn dismiss_help_window(app: AppHandle, complete_onboarding: bool) -> Result<(), String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    if complete_onboarding {
        db::write_onboarding_completed(&conn, true).map_err(|err| err.to_string())?;
    }
    service::hide_help_window(&app).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn submit_memory_review(
    app: AppHandle,
    action: String,
    summary: Option<String>,
) -> Result<Snapshot, String> {
    service::respond_memory_review(&app, &action, summary.as_deref()).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn set_active_view(app: AppHandle, view: AppView) -> Result<Snapshot, String> {
    let resolved = service::resolve_requested_view(&app, view).map_err(|err| err.to_string())?;
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::write_active_view(&conn, &resolved).map_err(|err| err.to_string())?;
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let scanning = service::current_scanning(&app);
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn save_article_note(
    app: AppHandle,
    article_id: i64,
    note: String,
) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::update_article_note(&conn, article_id, &note).map_err(|err| err.to_string())?;
    let source_id = db::article_source_id(&conn, article_id).map_err(|err| err.to_string())?;
    db::log_user_event(
        &conn,
        "note-updated",
        Some(article_id),
        source_id.as_deref(),
        Some(&format!(r#"{{"length":{}}}"#, note.chars().count())),
    )
    .map_err(|err| err.to_string())?;
    let settings = db::read_settings(&conn).map_err(|err| err.to_string())?;
    let _ = db::refresh_daily_memory(&conn, settings.memory_mode_enabled)
        .map_err(|err| err.to_string())?;
    let scanning = service::current_scanning(&app);
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn get_article_raw_content(app: AppHandle, article_id: i64) -> Result<String, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::fetch_article_raw_content(&conn, article_id).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_history_articles_page(
    app: AppHandle,
    offset: usize,
    limit: usize,
) -> Result<Vec<HistoryItem>, String> {
    db::list_push_history_articles_page(&app, offset, limit).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn add_custom_rss_source(
    app: AppHandle,
    name: String,
    url: String,
    module: String,
    bucket: String,
) -> Result<Snapshot, String> {
    let trimmed_name = name.trim();
    let trimmed_url = url.trim();
    if trimmed_name.is_empty() || trimmed_url.is_empty() {
        return Err("name and url are required".to_string());
    }

    let module = policy::normalize_module(&module);
    let bucket = policy::normalize_bucket(&module, &bucket);
    let canonical_url = db::canonicalize_source_url(trimmed_url);
    let source = RssSource {
        id: db::build_custom_source_id(trimmed_name, &canonical_url),
        name: trimmed_name.to_string(),
        url: canonical_url,
        module: module.clone(),
        bucket: bucket.clone(),
        discipline: map_module_to_discipline(&module),
        source_kind: map_bucket_to_source_kind(&bucket),
        resource_type: ResourceType::Article,
        language: None,
        enabled: true,
        enabled_by_default: true,
        postponed: false,
        origin_files: vec!["user-custom".to_string()],
    };

    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::upsert_source(&conn, &source, true).map_err(|err| err.to_string())?;
    db::reset_source_fetch_state(&conn, &source.id).map_err(|err| err.to_string())?;
    db::reset_module_fetch_state(&conn, &module).map_err(|err| err.to_string())?;

    let settings = db::read_settings(&conn).map_err(|err| err.to_string())?;
    let api_key_valid = db::read_api_key_valid(&conn).map_err(|err| err.to_string())?;
    if !service::requires_configuration(&settings, Some(api_key_valid)) {
        service::ensure_scheduler(&app);
        service::trigger_fetch_now(&app, Some(std::time::Duration::from_millis(250)), false);
    }

    let scanning = service::current_scanning(&app);
    service::sync_windows(&app, scanning).map_err(|err| err.to_string())?;
    service::publish_snapshot(&app, scanning).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn reset_runtime_data(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    db::hard_reset_all(&app).map_err(|err| err.to_string())?;
    service::clear_last_error(&app);
    if let Ok(mut scanning) = state.is_scanning.lock() {
        *scanning = false;
    }
    if let Ok(mut started) = state.scheduler_started.lock() {
        *started = false;
    }
    if let Ok(mut last_error) = state.last_error.lock() {
        *last_error = None;
    }
    if let Ok(mut api_key_valid) = state.api_key_valid.lock() {
        *api_key_valid = None;
    }
    if let Ok(mut last_scan_at) = state.last_scan_at.lock() {
        *last_scan_at = None;
    }
    if let Ok(mut loading_until) = state.loading_until.lock() {
        *loading_until = None;
    }
    if let Ok(mut pet_visible_until) = state.pet_visible_until.lock() {
        *pet_visible_until = None;
    }
    if let Ok(mut polling_until) = state.polling_until.lock() {
        *polling_until = None;
    }
    process::restart(&app.env());
    Ok(())
}

fn map_module_to_discipline(module: &str) -> Discipline {
    match module {
        "technology" => Discipline::Technology,
        "social_science" => Discipline::SocialScience,
        "business" => Discipline::Other,
        "growth" => Discipline::Life,
        "news_opinion" => Discipline::News,
        "entertainment" => Discipline::Humanities,
        "science" => Discipline::Science,
        "medicine" => Discipline::Medicine,
        _ => Discipline::Other,
    }
}

fn map_bucket_to_source_kind(bucket: &str) -> SourceKind {
    match bucket {
        "research" | "academic_frontier" | "physics" | "chemistry" | "biology" => {
            SourceKind::AcademicJournal
        }
        "official" => SourceKind::OfficialAnnouncement,
        "blogs" => SourceKind::TechnicalBlog,
        _ => SourceKind::CommunityHotspot,
    }
}

#[tauri::command]
pub fn reset_app_data(app: AppHandle, state: State<'_, AppState>) -> Result<Snapshot, String> {
    let conn = db::connect(&app).map_err(|err| err.to_string())?;
    db::reset_runtime_data(&conn).map_err(|err| err.to_string())?;
    db::reset_push_runtime_data(&app).map_err(|err| err.to_string())?;
    service::clear_last_error(&app);
    if let Ok(mut api_key_valid) = state.api_key_valid.lock() {
        *api_key_valid = Some(false);
    }
    if let Ok(mut last_scan_at) = state.last_scan_at.lock() {
        *last_scan_at = None;
    }
    if let Ok(mut loading_until) = state.loading_until.lock() {
        *loading_until = None;
    }
    if let Ok(mut scanning) = state.is_scanning.lock() {
        *scanning = false;
    }
    if let Ok(mut polling_until) = state.polling_until.lock() {
        *polling_until = None;
    }
    app.autolaunch().disable().map_err(|err| err.to_string())?;
    service::sync_windows(&app, false).map_err(|err| err.to_string())?;
    service::snapshot(&app, false).map_err(|err| err.to_string())
}
