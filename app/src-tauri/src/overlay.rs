use crate::dictation::DictationPhase;
use tauri::{AppHandle, Emitter, Manager};

/// Overlay is click-through and never takes keyboard focus.
pub fn prepare_overlay(app: &AppHandle) {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return;
    };
    let _ = overlay.set_focusable(false);
    let _ = overlay.set_ignore_cursor_events(true);
    let _ = overlay.set_shadow(false);
    let _ = overlay.hide();
}

fn emit_on_overlay(app: &AppHandle, event: &str, payload: impl serde::Serialize + Clone) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.emit(event, payload);
    }
}

pub fn sync_overlay(app: &AppHandle, phase: DictationPhase) {
    let phase_str = phase.as_str();
    emit_on_overlay(app, "overlay-phase", phase_str);
    let Some(overlay) = app.get_webview_window("overlay") else {
        return;
    };

    match phase {
        DictationPhase::Recording | DictationPhase::Transcribing => {
            let _ = overlay.center();
            let _ = overlay.show();
        }
        DictationPhase::Idle => {
            emit_on_overlay(app, "overlay-partial", "");
            let _ = overlay.hide();
        }
    }
}

pub fn emit_partial_text(app: &AppHandle, text: &str) {
    emit_on_overlay(app, "overlay-partial", text);
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.show();
    }
}
