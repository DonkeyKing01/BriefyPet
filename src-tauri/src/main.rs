#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod llm;
mod models;
mod policy;
mod rss;
mod service;
mod tray;

use std::sync::Mutex;

use tauri::{LogicalPosition, LogicalSize, Manager, WindowBuilder, WindowEvent, WindowUrl};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::models::AppView;

pub struct AppState {
    is_scanning: Mutex<bool>,
    scheduler_started: Mutex<bool>,
    last_error: Mutex<Option<String>>,
    api_key_valid: Mutex<Option<bool>>,
    last_scan_at: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    loading_until: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    pet_visible_until: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
    polling_until: Mutex<Option<chrono::DateTime<chrono::Utc>>>,
}

fn build_pet_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "pet", WindowUrl::App("index.html".into()))
        .title("Briefy Pet")
        .inner_size(188.0, 214.0)
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .position(32.0, 720.0)
        .visible(false)
        .build()?;
    Ok(())
}

fn build_bubble_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "bubble", WindowUrl::App("index.html".into()))
        .title("Briefy Pet Bubble")
        .inner_size(480.0, 360.0)
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

fn build_help_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "help", WindowUrl::App("index.html".into()))
        .title("Briefy Pet Help")
        .transparent(true)
        .decorations(false)
        .inner_size(560.0, 520.0)
        .min_inner_size(520.0, 480.0)
        .resizable(false)
        .visible(false)
        .always_on_top(true)
        .center()
        .build()?;
    Ok(())
}

fn build_memory_review_window(app: &tauri::App) -> tauri::Result<()> {
    WindowBuilder::new(app, "memory-review", WindowUrl::App("index.html".into()))
        .title("Briefy Pet Memory Review")
        .transparent(true)
        .decorations(false)
        .inner_size(740.0, 620.0)
        .min_inner_size(680.0, 560.0)
        .resizable(false)
        .visible(false)
        .always_on_top(true)
        .center()
        .build()?;
    Ok(())
}

fn position_overlay_windows(app: &tauri::App) -> tauri::Result<()> {
    let monitor_window = app
        .get_window("pet")
        .or_else(|| app.get_window("main"))
        .or_else(|| app.get_window("bubble"));
    let Some(window) = monitor_window else {
        return Ok(());
    };
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let logical_size = monitor.size().to_logical::<f64>(scale);
    let right_margin = 32.0;
    let bottom_margin = 36.0;
    let pet_width = 188.0;
    let pet_height = 214.0;
    let bubble_width = 480.0;
    let bubble_height = 360.0;

    let pet_x = (logical_size.width - pet_width - right_margin).max(0.0);
    let pet_y = (logical_size.height - pet_height - bottom_margin).max(0.0);
    let bubble_x = (pet_x + pet_width - bubble_width - 12.0).max(0.0);
    let bubble_y = (pet_y - bubble_height - 12.0).max(0.0);

    if let Some(window) = app.get_window("pet") {
        window.set_position(LogicalPosition::new(pet_x, pet_y))?;
    }
    if let Some(window) = app.get_window("bubble") {
        window.set_position(LogicalPosition::new(bubble_x, bubble_y))?;
    }
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
            pet_visible_until: Mutex::new(None),
            polling_until: Mutex::new(None),
        })
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None::<Vec<&str>>,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            } else if let Some(window) = app.get_window("pet") {
                let _ = window.show();
            }
        }))
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
            let persisted_api_key_valid = db::read_api_key_valid(&conn)?;
            if let Ok(mut api_key_valid) = app.state::<AppState>().api_key_valid.lock() {
                *api_key_valid = Some(persisted_api_key_valid);
            }
            let should_force_settings =
                service::requires_configuration(&settings, Some(persisted_api_key_valid));
            let onboarding_completed = db::read_onboarding_completed(&conn)?;
            if should_force_settings {
                db::write_active_view(&conn, &AppView::Settings)?;
            }
            if settings.auto_start {
                let _ = app.autolaunch().enable();
            } else {
                let _ = app.autolaunch().disable();
            }
            let should_scan = !should_force_settings;
            if !should_scan {
                if settings.api_key.trim().is_empty() || !persisted_api_key_valid {
                    if let Ok(mut api_key_valid) = app.state::<AppState>().api_key_valid.lock() {
                        *api_key_valid = Some(false);
                    }
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
            build_help_window(app)?;
            build_memory_review_window(app)?;
            position_overlay_windows(app)?;

            if let Some(help_window) = app.get_window("help") {
                let help_window_handle = help_window.clone();
                help_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = help_window_handle.hide();
                    }
                });
            }

            if let Some(memory_window) = app.get_window("memory-review") {
                let app_handle = app.handle();
                let memory_window_handle = memory_window.clone();
                memory_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = memory_window_handle.show();
                        let _ = memory_window_handle.set_focus();
                        let _ = service::sync_windows(
                            &app_handle,
                            service::current_scanning(&app_handle),
                        );
                    }
                });
            }

            if should_scan {
                service::reveal_pet_on_launch(&app.handle(), 6)?;
                service::ensure_scheduler(&app.handle());
                service::trigger_fetch_now(
                    &app.handle(),
                    Some(std::time::Duration::from_secs(3)),
                    true,
                );
            } else {
                service::reveal_pet_on_launch(&app.handle(), 6)?;
                service::sync_windows(&app.handle(), false)?;
            }

            if !onboarding_completed {
                service::show_help_window(&app.handle())?;
            }

            Ok(())
        })
        .system_tray(tray::create_tray())
        .on_system_tray_event(tray::handle_tray_event)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::bootstrap_overlay,
            commands::save_settings,
            commands::open_article,
            commands::toggle_favorite,
            commands::pet_double_click,
            commands::bubble_action,
            commands::open_help_window,
            commands::dismiss_help_window,
            commands::submit_memory_review,
            commands::set_active_view,
            commands::save_article_note,
            commands::get_article_raw_content,
            commands::list_history_articles_page,
            commands::add_custom_rss_source,
            commands::reset_runtime_data
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
