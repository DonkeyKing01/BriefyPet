#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod diagnostics;
mod llm;
mod models;
mod policy;
mod rss;
mod service;
mod tray;

use std::sync::Mutex;

use tauri::{LogicalPosition, LogicalSize, Manager, WindowBuilder, WindowEvent, WindowUrl};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

pub struct AppState {
    is_scanning: Mutex<bool>,
    scheduler_started: Mutex<bool>,
    last_error: Mutex<Option<String>>,
    api_key_valid: Mutex<Option<bool>>,
    last_scan_at: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    loading_until: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

fn build_pet_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "pet", WindowUrl::App("index.html".into()))
        .title("Briefy Pet")
        .inner_size(188.0, 196.0)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .position(32.0, 720.0)
        .visible(true)
        .build()?;
    Ok(())
}

fn build_bubble_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "bubble", WindowUrl::App("index.html".into()))
        .title("Briefy Pet Bubble")
        .inner_size(380.0, 260.0)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .position(240.0, 620.0)
        .visible(false)
        .build()?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            is_scanning: Mutex::new(false),
            scheduler_started: Mutex::new(false),
            last_error: Mutex::new(None),
            api_key_valid: Mutex::new(None),
            last_scan_at: Mutex::new(None),
            loading_until: Mutex::new(None),
        })
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .setup(|app| -> Result<(), Box<dyn std::error::Error>> {
            if let Some(main_window) = app.get_window("main") {
                main_window.set_size(LogicalSize::new(1260.0, 780.0))?;
                main_window.set_position(LogicalPosition::new(220.0, 120.0))?;
                let main_window_handle = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = main_window_handle.hide();
                    }
                });
            }

            let conn = db::connect(&app.handle())?;
            let settings = db::read_settings(&conn)?;
            diagnostics::log(
                &app.handle(),
                "startup",
                format!(
                    "app boot: auto_start={} api_key_present={} selected_disciplines={}",
                    settings.auto_start,
                    !settings.api_key.trim().is_empty(),
                    settings.disciplines.iter().filter(|item| item.enabled).count()
                ),
            );
            if settings.auto_start {
                let _ = app.autolaunch().enable();
            } else {
                let _ = app.autolaunch().disable();
            }
            let should_scan = !settings.api_key.trim().is_empty();
            if !should_scan {
                if let Ok(mut api_key_valid) = app.state::<AppState>().api_key_valid.lock() {
                    *api_key_valid = Some(false);
                }
            }
            {
                let state = app.state::<AppState>();
                let mut flag = state
                    .is_scanning
                    .lock()
                    .map_err(|err| std::io::Error::other(err.to_string()))?;
                *flag = false;
            }

            if should_scan {
                if let Ok(mut loading_until) = app.state::<AppState>().loading_until.lock() {
                    *loading_until = Some(chrono::Utc::now() + chrono::Duration::seconds(3));
                }
            } else if let Ok(mut loading_until) = app.state::<AppState>().loading_until.lock() {
                *loading_until = None;
            }

            build_pet_window(app)?;
            build_bubble_window(app)?;

            if should_scan {
                diagnostics::log(
                    &app.handle(),
                    "startup",
                    "scheduler enabled and initial fetch scheduled in 3 seconds",
                );
                service::ensure_scheduler(&app.handle());
                service::trigger_fetch_now(&app.handle(), Some(std::time::Duration::from_secs(3)));
            } else {
                diagnostics::log(
                    &app.handle(),
                    "startup",
                    "initial fetch skipped because api key is missing",
                );
                service::sync_windows(&app.handle(), false)?;
            }

            Ok(())
        })
        .system_tray(tray::create_tray())
        .on_system_tray_event(tray::handle_tray_event)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_settings,
            commands::open_article,
            commands::toggle_favorite,
            commands::pet_double_click,
            commands::bubble_action,
            commands::set_active_view,
            commands::reset_app_data
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
