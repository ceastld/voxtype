use crate::audio_capture::AudioCapture;
use crate::runtime_process::RuntimeProcess;
use crate::settings::{load_settings, save_settings, AppSettings};
use crate::text_output::type_unicode;
use crate::voice_ws::{check_runtime_health, VoiceWsSession};
use parking_lot::RwLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationPhase {
    Idle,
    Recording,
    Transcribing,
}

impl DictationPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
        }
    }
}

enum DictationCommand {
    Start,
    Stop,
    RestartRuntime,
}

struct ActiveRecording {
    audio: AudioCapture,
    ws: VoiceWsSession,
}

/// Send-safe facade; audio capture lives on a dedicated worker thread.
pub struct DictationHandle {
    tx: Sender<DictationCommand>,
    phase: Arc<RwLock<DictationPhase>>,
    runtime: Arc<RuntimeProcess>,
    app: Arc<parking_lot::Mutex<Option<AppHandle>>>,
    _worker: JoinHandle<()>,
}

impl DictationHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let phase = Arc::new(RwLock::new(DictationPhase::Idle));
        let runtime = Arc::new(RuntimeProcess::new());
        let app: Arc<parking_lot::Mutex<Option<AppHandle>>> =
            Arc::new(parking_lot::Mutex::new(None));
        let phase_worker = Arc::clone(&phase);
        let runtime_worker = Arc::clone(&runtime);
        let app_worker = Arc::clone(&app);

        let worker = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("dictation runtime");
            let mut active: Option<ActiveRecording> = None;

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    DictationCommand::Start => {
                        let _ = rt.block_on(start_recording(
                            &runtime_worker,
                            &phase_worker,
                            &app_worker,
                            &mut active,
                        ));
                    }
                    DictationCommand::Stop => {
                        let _ = rt.block_on(stop_recording_and_type(
                            &phase_worker,
                            &app_worker,
                            &mut active,
                        ));
                    }
                    DictationCommand::RestartRuntime => {
                        runtime_worker.stop();
                        let _ = runtime_worker.start();
                    }
                }
            }
        });

        Self {
            tx,
            phase,
            runtime,
            app,
            _worker: worker,
        }
    }

    pub fn set_app(&self, app: AppHandle) {
        *self.app.lock() = Some(app);
    }

    pub fn phase(&self) -> DictationPhase {
        *self.phase.read()
    }

    pub fn runtime_running(&self) -> bool {
        self.runtime.is_running()
    }

    pub async fn runtime_ready(&self) -> bool {
        let settings = load_settings();
        check_runtime_health(settings.runtime_ws_port).await
    }

    pub async fn ensure_runtime(&self) -> Result<(), String> {
        let settings = load_settings();
        if !self.runtime.is_running() {
            self.runtime.start()?;
        }
        let ok = RuntimeProcess::wait_until_healthy(settings.runtime_ws_port, 45_000).await;
        if !ok {
            return Err("识别服务启动超时".into());
        }
        Ok(())
    }

    pub fn restart_runtime(&self) -> Result<(), String> {
        self.tx
            .send(DictationCommand::RestartRuntime)
            .map_err(|e| e.to_string())
    }

    pub fn start_recording(&self) -> Result<(), String> {
        self.tx
            .send(DictationCommand::Start)
            .map_err(|e| e.to_string())
    }

    pub fn stop_recording_and_type(&self) -> Result<(), String> {
        self.tx
            .send(DictationCommand::Stop)
            .map_err(|e| e.to_string())
    }

    pub async fn toggle(&self) -> Result<(), String> {
        match self.phase() {
            DictationPhase::Idle => {
                self.ensure_runtime().await?;
                self.start_recording()?;
            }
            DictationPhase::Recording => {
                self.stop_recording_and_type()?;
            }
            DictationPhase::Transcribing => {}
        }
        Ok(())
    }
}

fn emit_overlay(app: &Arc<parking_lot::Mutex<Option<AppHandle>>>, phase: DictationPhase) {
    if let Some(handle) = app.lock().clone() {
        let _ = handle.emit("overlay-phase", phase.as_str());
    }
}

async fn start_recording(
    runtime: &RuntimeProcess,
    phase: &Arc<RwLock<DictationPhase>>,
    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,
    active: &mut Option<ActiveRecording>,
) -> Result<(), String> {
    if active.is_some() {
        return Ok(());
    }
    let settings = load_settings();
    if !runtime.is_running() {
        runtime.start()?;
    }
    let ok = RuntimeProcess::wait_until_healthy(settings.runtime_ws_port, 45_000).await;
    if !ok {
        return Err("识别服务启动超时".into());
    }

    let ws = VoiceWsSession::connect(settings.runtime_ws_port).await?;
    ws.start_session().await?;
    let audio = AudioCapture::start()?;
    *active = Some(ActiveRecording { audio, ws });
    *phase.write() = DictationPhase::Recording;
    emit_overlay(app, DictationPhase::Recording);
    Ok(())
}

async fn stop_recording_and_type(
    phase: &Arc<RwLock<DictationPhase>>,
    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,
    active: &mut Option<ActiveRecording>,
) -> Result<String, String> {
    if active.is_none() {
        return Ok(String::new());
    }
    *phase.write() = DictationPhase::Transcribing;
    emit_overlay(app, DictationPhase::Transcribing);

    let Some(rec) = active.take() else {
        *phase.write() = DictationPhase::Idle;
        emit_overlay(app, DictationPhase::Idle);
        return Ok(String::new());
    };

    let pcm = rec.audio.drain_all_pcm_bytes();
    if pcm.len() >= 6400 {
        rec.ws.send_pcm(&pcm);
    }

    let result = rec.ws.end_session().await?;
    let mut text = result.text.trim().to_string();

    let settings = load_settings();
    if settings.strip_trailing_punctuation {
        text = strip_trailing_punct(&text);
    }

    if !text.is_empty() {
        type_unicode(&text)?;
    }

    *phase.write() = DictationPhase::Idle;
    emit_overlay(app, DictationPhase::Idle);
    Ok(text)
}

fn strip_trailing_punct(text: &str) -> String {
    let mut s = text.to_string();
    while let Some(c) = s.chars().last() {
        if "，。！？、；：,.!?;:".contains(c) {
            s.pop();
        } else {
            break;
        }
    }
    s
}

pub fn update_hotkey_setting(hotkey: &str) -> Result<(), String> {
    let mut s = load_settings();
    s.hotkey = hotkey.trim().to_string();
    save_settings(&s)
}

pub async fn build_status(handle: &DictationHandle) -> serde_json::Value {
    let settings: AppSettings = load_settings();
    let ready = handle.runtime_ready().await;
    let active_name = settings.active_model_id.as_ref().and_then(|id| {
        crate::settings::load_catalog()
            .ok()
            .and_then(|c| c.models.into_iter().find(|m| &m.id == id))
            .map(|m| m.name)
    });
    serde_json::json!({
        "runtimeRunning": handle.runtime_running(),
        "runtimeReady": ready,
        "dictationPhase": handle.phase().as_str(),
        "activeModelId": settings.active_model_id,
        "activeModelName": active_name,
        "hotkey": settings.hotkey,
    })
}

// Keep receiver type used
#[allow(dead_code)]
fn _rx_hint(_: Receiver<DictationCommand>) {}
