mod audio_capture;
mod dictation;
mod http_api;
mod model_download;
mod runtime_process;
mod settings;
mod text_output;
mod voice_ws;

use dictation::DictationHandle;
use http_api::ApiState;
use settings::load_settings;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct AppState {
    dictation: Arc<DictationHandle>,
}

#[tauri::command]
async fn get_app_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(dictation::build_status(state.inner().dictation.as_ref()).await)
}

#[tauri::command]
fn load_models_catalog() -> Result<settings::ModelsCatalog, String> {
    settings::load_catalog()
}

#[tauri::command]
fn list_models_status() -> Result<Vec<settings::ModelStatusDto>, String> {
    settings::list_model_statuses()
}

#[tauri::command]
fn set_hotkey(hotkey: String) -> Result<(), String> {
    dictation::update_hotkey_setting(&hotkey)
}

#[tauri::command]
async fn download_model(app: AppHandle, model_id: String) -> Result<(), String> {
    model_download::download_model(&app, &model_id).await
}

#[tauri::command]
fn activate_model(model_id: String, state: State<'_, AppState>) -> Result<(), String> {
    model_download::activate_model(&model_id)?;
    state.inner().dictation.restart_runtime()
}

#[tauri::command]
fn restart_runtime(state: State<'_, AppState>) -> Result<(), String> {
    state.inner().dictation.restart_runtime()
}

fn register_hotkey(app: &AppHandle, dictation: Arc<DictationHandle>) -> Result<(), String> {
    let settings = load_settings();
    let hotkey = settings
        .hotkey
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("无效热键 {}: {e}", settings.hotkey))?;

    app.global_shortcut()
        .on_shortcut(hotkey, move |_app, _shortcut, event| {
            let dictation = Arc::clone(&dictation);
            if event.state == ShortcutState::Pressed {
                let _ = dictation.start_recording();
            } else if event.state == ShortcutState::Released {
                let _ = dictation.stop_recording_and_type();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let dictation = Arc::new(DictationHandle::new());

    tauri::async_runtime::block_on({
        let d = Arc::clone(&dictation);
        async move {
            if let Err(e) = d.ensure_runtime().await {
                tracing::warn!("runtime autostart: {e}");
            }
        }
    });

    let api_state = Arc::new(ApiState {
        dictation: Arc::clone(&dictation),
    });
    let api_port = load_settings().api_port;
    tauri::async_runtime::spawn(async move {
        http_api::serve(api_state, api_port).await;
    });

    let dictation_for_setup = Arc::clone(&dictation);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            dictation: Arc::clone(&dictation),
        })
        .setup(move |app| {
            dictation_for_setup.set_app(app.handle().clone());

            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.show();
            }

            let show_item = MenuItem::with_id(app, "show", "设置", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            register_hotkey(&app.handle().clone(), Arc::clone(&dictation_for_setup))?;
            let _ = app.emit("overlay-phase", "idle");
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            load_models_catalog,
            list_models_status,
            set_hotkey,
            download_model,
            activate_model,
            restart_runtime,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
