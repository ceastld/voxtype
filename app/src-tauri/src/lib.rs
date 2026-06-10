mod audio_capture;
mod dictation;
mod hotkey;
mod http_api;
mod model_download;
mod overlay;
mod runtime_process;
mod settings;
mod text_output;
mod voice_ws;

use dictation::DictationHandle;
use hotkey::HotkeyManager;
use http_api::ApiState;
use settings::load_settings;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

struct AppState {
    dictation: Arc<DictationHandle>,
    hotkey: Arc<HotkeyManager>,
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
fn validate_hotkey(hotkey: String) -> Result<(), String> {
    hotkey::HotkeyManager::validate(&hotkey)
}

#[tauri::command]
fn set_hotkey(
    hotkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    hotkey::HotkeyManager::validate(&hotkey)?;
    dictation::update_hotkey_setting(&hotkey)?;
    state
        .inner()
        .hotkey
        .register(&app, Arc::clone(&state.inner().dictation))
}

#[tauri::command]
fn set_hotkey_mode(
    mode: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    dictation::update_hotkey_mode_setting(&mode)?;
    state
        .inner()
        .hotkey
        .register(&app, Arc::clone(&state.inner().dictation))
}

#[tauri::command]
fn set_use_gpu(use_gpu: bool, state: State<'_, AppState>) -> Result<(), String> {
    dictation::update_use_gpu_setting(use_gpu)?;
    state.inner().dictation.restart_runtime()
}

#[tauri::command]
async fn download_model(
    app: AppHandle,
    model_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    model_download::download_model(&app, &model_id).await?;
    let _ = state.inner().dictation.restart_runtime();
    Ok(())
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

#[tauri::command]
async fn dictation_start(state: State<'_, AppState>) -> Result<(), String> {
    state.inner().dictation.start_recording().await
}

#[tauri::command]
async fn dictation_stop(state: State<'_, AppState>) -> Result<String, String> {
    state.inner().dictation.stop_recording_and_type().await
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
    let hotkey_manager = Arc::new(HotkeyManager::new());
    let hotkey_for_setup = Arc::clone(&hotkey_manager);

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState {
            dictation: Arc::clone(&dictation),
            hotkey: Arc::clone(&hotkey_manager),
        })
        .setup(move |app| {
            dictation_for_setup.set_app(app.handle().clone());

            let dictation_watch = Arc::clone(&dictation_for_setup);
            let app_watch = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_ready: Option<bool> = None;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let ready = dictation_watch.runtime_ready().await;
                    if last_ready != Some(ready) {
                        last_ready = Some(ready);
                        let _ = app_watch.emit(
                            "runtime-status-changed",
                            serde_json::json!({ "ready": ready }),
                        );
                    }
                }
            });

            overlay::prepare_overlay(&app.handle());

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

            if let Err(e) = hotkey_for_setup.register(
                &app.handle().clone(),
                Arc::clone(&dictation_for_setup),
            ) {
                tracing::warn!("global hotkey disabled: {e}");
            }
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
            validate_hotkey,
            set_hotkey,
            set_hotkey_mode,
            set_use_gpu,
            download_model,
            activate_model,
            restart_runtime,
            dictation_start,
            dictation_stop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
