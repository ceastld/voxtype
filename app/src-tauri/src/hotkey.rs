use crate::dictation::DictationHandle;
use crate::settings::load_settings;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub struct HotkeyManager {
    active: Mutex<Option<String>>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }

    pub fn validate(hotkey: &str) -> Result<(), String> {
        let trimmed = hotkey.trim();
        if trimmed.is_empty() {
            return Err("快捷键不能为空".into());
        }
        trimmed
            .parse::<Shortcut>()
            .map(|_| ())
            .map_err(|e| format!("无效快捷键 {trimmed}: {e}"))
    }

    pub fn register(
        &self,
        app: &AppHandle,
        dictation: Arc<DictationHandle>,
    ) -> Result<(), String> {
        let _ = app.global_shortcut().unregister_all();
        *self.active.lock() = None;

        let settings = load_settings();
        let hotkey_str = settings.hotkey.trim().to_string();
        Self::validate(&hotkey_str)?;

        let shortcut: Shortcut = hotkey_str
            .parse()
            .map_err(|e| format!("无效快捷键 {hotkey_str}: {e}"))?;
        let mode = settings.hotkey_mode.clone();

        app.global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                if mode == "toggle" {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let dictation = Arc::clone(&dictation);
                    tauri::async_runtime::spawn(async move {
                        let _ = dictation.toggle().await;
                    });
                    return;
                }

                let dictation = Arc::clone(&dictation);
                if event.state == ShortcutState::Pressed {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = dictation.hold_begin().await {
                            tracing::warn!("hotkey start: {e}");
                        }
                    });
                } else if event.state == ShortcutState::Released {
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = dictation.hold_end().await {
                            tracing::warn!("hotkey stop: {e}");
                        }
                    });
                }
            })
            .map_err(|e| format!("注册快捷键失败（可能被其他程序占用）: {e}"))?;

        *self.active.lock() = Some(hotkey_str);
        Ok(())
    }
}
