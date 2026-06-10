use crate::dictation::DictationHandle;
use crate::settings::load_settings;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Short press+release within this window is treated as toggle (tap twice to finish).
const AUTO_TAP_THRESHOLD_MS: u64 = 350;

struct AutoHotkeyState {
    press_started: Mutex<Option<Instant>>,
    tap_armed: AtomicBool,
}

impl AutoHotkeyState {
    fn new() -> Self {
        Self {
            press_started: Mutex::new(None),
            tap_armed: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        *self.press_started.lock() = None;
        self.tap_armed.store(false, Ordering::Release);
    }
}

pub struct HotkeyManager {
    active: Mutex<Option<String>>,
    auto_state: Arc<AutoHotkeyState>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
            auto_state: Arc::new(AutoHotkeyState::new()),
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
        self.auto_state.reset();

        let settings = load_settings();
        let hotkey_str = settings.hotkey.trim().to_string();
        Self::validate(&hotkey_str)?;

        let shortcut: Shortcut = hotkey_str
            .parse()
            .map_err(|e| format!("无效快捷键 {hotkey_str}: {e}"))?;
        let mode = settings.hotkey_mode.clone();
        let auto_state = Arc::clone(&self.auto_state);

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

                if mode == "auto" {
                    handle_auto_hotkey(&dictation, &auto_state, event.state);
                    return;
                }

                // hold
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

fn handle_auto_hotkey(
    dictation: &Arc<DictationHandle>,
    auto_state: &Arc<AutoHotkeyState>,
    state: ShortcutState,
) {
    let dictation = Arc::clone(dictation);
    let auto_state = Arc::clone(auto_state);

    if state == ShortcutState::Pressed {
        if auto_state.tap_armed.load(Ordering::Acquire) {
            auto_state.tap_armed.store(false, Ordering::Release);
            tauri::async_runtime::spawn(async move {
                if let Err(e) = dictation.toggle().await {
                    tracing::warn!("hotkey auto tap stop: {e}");
                }
            });
            return;
        }

        if auto_state.press_started.lock().is_some() {
            return;
        }

        *auto_state.press_started.lock() = Some(Instant::now());
        tauri::async_runtime::spawn(async move {
            if let Err(e) = dictation.hold_begin().await {
                tracing::warn!("hotkey auto start: {e}");
                auto_state.reset();
            }
        });
        return;
    }

    if state != ShortcutState::Released {
        return;
    }

    let Some(started) = auto_state.press_started.lock().take() else {
        return;
    };

    let elapsed = started.elapsed();
    if elapsed >= Duration::from_millis(AUTO_TAP_THRESHOLD_MS) {
        auto_state.tap_armed.store(false, Ordering::Release);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = dictation.hold_end().await {
                tracing::warn!("hotkey auto hold stop: {e}");
            }
        });
    } else {
        auto_state.tap_armed.store(true, Ordering::Release);
    }
}
