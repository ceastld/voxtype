use crate::audio_capture::AudioCapture;

use crate::runtime_process::RuntimeProcess;

use crate::settings::{load_settings, save_settings, AppSettings};

use crate::text_output::type_unicode;

use crate::voice_ws::{fetch_runtime_health, VoiceWsSession};

use parking_lot::RwLock;

use serde_json::json;

use std::sync::mpsc::{self, Receiver, SyncSender};

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;



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

    Start(SyncSender<Result<(), String>>),

    Stop(SyncSender<Result<String, String>>),

    RestartRuntime,

}



struct ActiveRecording {
    audio: AudioCapture,
    ws: VoiceWsSession,
    bytes_streamed: Arc<AtomicUsize>,
    stop_stream: Option<std_mpsc::Sender<()>>,
    stream_handle: Option<JoinHandle<()>>,
}



/// Send-safe facade; audio capture lives on a dedicated worker thread.

pub struct DictationHandle {

    tx: mpsc::Sender<DictationCommand>,

    phase: Arc<RwLock<DictationPhase>>,

    last_error: Arc<RwLock<Option<String>>>,

    last_text: Arc<RwLock<Option<String>>>,

    runtime: Arc<RuntimeProcess>,

    app: Arc<parking_lot::Mutex<Option<AppHandle>>>,

    hold_start_in_flight: Arc<AtomicBool>,

    hold_start_done: Arc<Notify>,

    _worker: JoinHandle<()>,

}



impl DictationHandle {

    pub fn new() -> Self {

        let (tx, rx) = mpsc::channel();

        let phase = Arc::new(RwLock::new(DictationPhase::Idle));

        let last_error = Arc::new(RwLock::new(None));

        let last_text = Arc::new(RwLock::new(None));

        let runtime = Arc::new(RuntimeProcess::new());

        let app: Arc<parking_lot::Mutex<Option<AppHandle>>> =

            Arc::new(parking_lot::Mutex::new(None));

        let phase_worker = Arc::clone(&phase);

        let last_error_worker = Arc::clone(&last_error);

        let last_text_worker = Arc::clone(&last_text);

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

                    DictationCommand::Start(reply) => {

                        let result = rt.block_on(start_recording(

                            &runtime_worker,

                            &phase_worker,

                            &last_error_worker,

                            &app_worker,

                            &mut active,

                        ));

                        let _ = reply.send(result);

                    }

                    DictationCommand::Stop(reply) => {

                        let result = rt.block_on(stop_recording_and_type(

                            &phase_worker,

                            &last_error_worker,

                            &last_text_worker,

                            &app_worker,

                            &mut active,

                        ));

                        let _ = reply.send(result);

                    }

                    DictationCommand::RestartRuntime => {
                        runtime_worker.stop();
                        if let Err(e) = runtime_worker.ensure_started() {

                            set_error(&last_error_worker, &app_worker, e);

                        }

                    }

                }

            }

        });



        Self {
            tx,
            phase,
            last_error,
            last_text,
            runtime,
            app,
            hold_start_in_flight: Arc::new(AtomicBool::new(false)),
            hold_start_done: Arc::new(Notify::new()),
            _worker: worker,
        }

    }



    pub fn set_app(&self, app: AppHandle) {

        *self.app.lock() = Some(app);

    }



    pub fn phase(&self) -> DictationPhase {

        *self.phase.read()

    }



    pub fn last_error(&self) -> Option<String> {

        self.last_error.read().clone()

    }



    pub fn last_text(&self) -> Option<String> {

        self.last_text.read().clone()

    }



    pub fn runtime_running(&self) -> bool {

        self.runtime.is_running()

    }



    pub async fn runtime_ready(&self) -> bool {
        fetch_runtime_health(self.runtime.active_port())
            .await
            .map(|h| h.ready)
            .unwrap_or(false)

    }



    pub async fn ensure_runtime(&self) -> Result<(), String> {

        let log = crate::settings::runtime_log_path();

        let mut port = self.runtime.ensure_started()?;

        RuntimeProcess::wait_until_healthy(port, 90_000)
            .await
            .map_err(|e| format!("{e}（日志: {}）", log.display()))?;

        if let Err(ws_err) = verify_voice_socket(port).await {
            tracing::warn!("voice ws check failed on {port}: {ws_err}; switching port");
            port = self.runtime.restart_on_fresh_port()?;
            RuntimeProcess::wait_until_healthy(port, 90_000)
                .await
                .map_err(|e| format!("{e}（日志: {}）", log.display()))?;
            verify_voice_socket(port)
                .await
                .map_err(|e| format!("{e}（日志: {}）", log.display()))?;
        }

        Ok(())
    }



    pub fn restart_runtime(&self) -> Result<(), String> {

        self.tx

            .send(DictationCommand::RestartRuntime)

            .map_err(|e| e.to_string())

    }



    pub async fn start_recording(&self) -> Result<(), String> {

        let (tx, rx) = mpsc::sync_channel(1);

        self.tx

            .send(DictationCommand::Start(tx))

            .map_err(|e| e.to_string())?;

        tokio::task::spawn_blocking(move || {

            rx.recv()

                .unwrap_or_else(|_| Err("听写线程已退出".into()))

        })

        .await

        .map_err(|e| e.to_string())?

    }



    pub async fn stop_recording_and_type(&self) -> Result<String, String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .send(DictationCommand::Stop(tx))
            .map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || {
            rx.recv()
                .unwrap_or_else(|_| Err("听写线程已退出".into()))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Hold-to-talk: wait for press-side startup before stopping.
    pub async fn hold_begin(&self) -> Result<(), String> {
        self.hold_start_in_flight.store(true, Ordering::SeqCst);
        let result = async {
            self.ensure_runtime().await?;
            self.start_recording().await
        }
        .await;
        self.hold_start_in_flight.store(false, Ordering::SeqCst);
        self.hold_start_done.notify_waiters();
        result
    }

    pub async fn hold_end(&self) -> Result<String, String> {
        while self.hold_start_in_flight.load(Ordering::SeqCst) {
            let _ = tokio::time::timeout(
                Duration::from_secs(45),
                self.hold_start_done.notified(),
            )
            .await;
        }
        if self.phase() != DictationPhase::Recording {
            return Ok(String::new());
        }
        self.stop_recording_and_type().await
    }

    pub async fn toggle(&self) -> Result<(), String> {

        match self.phase() {

            DictationPhase::Idle => {

                self.ensure_runtime().await?;

                self.start_recording().await?;

            }

            DictationPhase::Recording => {

                let _ = self.stop_recording_and_type().await?;

            }

            DictationPhase::Transcribing => {}

        }

        Ok(())

    }

}



fn set_error(

    last_error: &Arc<RwLock<Option<String>>>,

    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,

    message: String,

) {

    tracing::error!("dictation: {message}");

    *last_error.write() = Some(message.clone());

    emit_status(app, DictationPhase::Idle, Some(message), None);

}



fn emit_status(

    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,

    phase: DictationPhase,

    error: Option<String>,

    text: Option<String>,

) {

    if let Some(handle) = app.lock().clone() {

        crate::overlay::sync_overlay(&handle, phase);

        let _ = handle.emit(

            "dictation-status",

            json!({

                "phase": phase.as_str(),

                "error": error,

                "text": text,

            }),

        );

    }

}



async fn start_recording(

    runtime: &RuntimeProcess,

    phase: &Arc<RwLock<DictationPhase>>,

    last_error: &Arc<RwLock<Option<String>>>,

    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,

    active: &mut Option<ActiveRecording>,

) -> Result<(), String> {

    *last_error.write() = None;



    if active.is_some() {

        return Ok(());

    }



    let port = runtime.ensure_started()?;

    RuntimeProcess::wait_until_healthy(port, 90_000)
        .await

        .map_err(|e| {

            set_error(last_error, app, e.clone());

            e

        })?;



    let app_handle = app.lock().clone();
    let ws = VoiceWsSession::connect(port, app_handle)
        .await
        .map_err(|e| {
            set_error(last_error, app, e.clone());
            e
        })?;

    ws.start_session().await.map_err(|e| {

        set_error(last_error, app, e.clone());

        e

    })?;



    let audio = AudioCapture::start().map_err(|e| {

        set_error(last_error, app, e.clone());

        e

    })?;



    let pcm_buf = audio.pcm_buffer();
    let ws_stream = ws.clone();
    let bytes_streamed = Arc::new(AtomicUsize::new(0));
    let bytes_counter = Arc::clone(&bytes_streamed);
    let (stop_tx, stop_rx) = std_mpsc::channel::<()>();
    let stream_handle = thread::spawn(move || {
        while stop_rx.try_recv().is_err() {
            thread::sleep(Duration::from_millis(200));
            let pcm: Vec<u8> = {
                let samples = std::mem::take(&mut *pcm_buf.lock().unwrap());
                samples
                    .into_iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect()
            };
            if !pcm.is_empty() {
                bytes_counter.fetch_add(pcm.len(), Ordering::Relaxed);
                ws_stream.send_pcm(&pcm);
            }
        }
    });

    *active = Some(ActiveRecording {
        audio,
        ws,
        bytes_streamed,
        stop_stream: Some(stop_tx),
        stream_handle: Some(stream_handle),
    });

    *phase.write() = DictationPhase::Recording;

    emit_status(app, DictationPhase::Recording, None, None);

    tracing::info!("dictation recording started");

    Ok(())

}



async fn stop_recording_and_type(

    phase: &Arc<RwLock<DictationPhase>>,

    last_error: &Arc<RwLock<Option<String>>>,

    last_text: &Arc<RwLock<Option<String>>>,

    app: &Arc<parking_lot::Mutex<Option<AppHandle>>>,

    active: &mut Option<ActiveRecording>,

) -> Result<String, String> {

    if active.is_none() {

        return Ok(String::new());

    }

    *phase.write() = DictationPhase::Transcribing;

    emit_status(app, DictationPhase::Transcribing, None, None);



    let Some(mut rec) = active.take() else {
        *phase.write() = DictationPhase::Idle;
        emit_status(app, DictationPhase::Idle, None, None);
        return Ok(String::new());
    };

    if let Some(stop) = rec.stop_stream.take() {
        let _ = stop.send(());
    }
    if let Some(handle) = rec.stream_handle.take() {
        let _ = handle.join();
    }

    let tail_pcm = rec.audio.drain_all_pcm_bytes();
    let streamed = rec.bytes_streamed.load(Ordering::Relaxed);
    let total_bytes = streamed + tail_pcm.len();
    let total_ms = (total_bytes as u64 * 1000) / (16_000 * 2);

    tracing::info!(
        "dictation audio total {total_ms}ms (streamed={streamed} tail={} bytes)",
        tail_pcm.len()
    );

    if !tail_pcm.is_empty() {
        rec.ws.send_pcm(&tail_pcm);
    }

    let result = rec.ws.end_session().await.map_err(|e| {

        set_error(last_error, app, e.clone());

        e

    })?;

    let mut text = result.text.trim().to_string();

    let settings = load_settings();
    if settings.strip_trailing_punctuation {
        text = strip_trailing_punct(&text);
    }

    if total_bytes < 3200 {
        let msg = format!("录音太短（约 {total_ms}ms），请按住多说一会");
        set_error(last_error, app, msg.clone());
        *phase.write() = DictationPhase::Idle;
        emit_status(app, DictationPhase::Idle, Some(msg.clone()), None);
        return Err(msg);
    }

    if text.is_empty() {
        let msg = "未识别到语音，请检查麦克风或靠近话筒再试".to_string();

        set_error(last_error, app, msg.clone());

        *phase.write() = DictationPhase::Idle;

        emit_status(app, DictationPhase::Idle, Some(msg.clone()), None);

        return Err(msg);

    }



    type_unicode(&text).map_err(|e| {

        set_error(last_error, app, e.clone());

        e

    })?;



    *last_text.write() = Some(text.clone());

    *phase.write() = DictationPhase::Idle;

    emit_status(app, DictationPhase::Idle, None, Some(text.clone()));

    tracing::info!("dictation result: {text}");

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



pub fn update_hotkey_mode_setting(mode: &str) -> Result<(), String> {

    let normalized = mode.trim().to_lowercase();

    if normalized != "hold" && normalized != "toggle" {

        return Err("hotkeyMode 必须是 hold 或 toggle".into());

    }

    let mut s = load_settings();

    s.hotkey_mode = normalized;

    save_settings(&s)

}



pub fn update_use_gpu_setting(use_gpu: bool) -> Result<(), String> {

    let mut s = load_settings();

    s.use_gpu = use_gpu;

    save_settings(&s)

}



pub async fn build_status(handle: &DictationHandle) -> serde_json::Value {

    let settings: AppSettings = load_settings();

    let health = fetch_runtime_health(handle.runtime.active_port()).await;

    let ready = health.as_ref().map(|h| h.ready).unwrap_or(false);

    let active_name = settings.active_model_id.as_ref().and_then(|id| {

        crate::settings::load_catalog()

            .ok()

            .and_then(|c| c.models.into_iter().find(|m| &m.id == id))

            .map(|m| m.name)

    });

    serde_json::json!({

        "runtimeRunning": handle.runtime_running(),

        "runtimeReady": ready,

        "runtimeWsPort": handle.runtime.active_port(),

        "runtimeHealth": health,

        "dictationPhase": handle.phase().as_str(),

        "lastError": handle.last_error(),

        "lastText": handle.last_text(),

        "activeModelId": settings.active_model_id,

        "activeModelName": active_name,

        "hotkey": settings.hotkey,

        "hotkeyMode": settings.hotkey_mode,

        "useGpu": settings.use_gpu,

        "requestedProvider": crate::settings::resolve_runtime_provider(settings.use_gpu),

        "runtimeExe": crate::settings::runtime_exe_path().to_string_lossy(),

        "runtimeLog": crate::settings::runtime_log_path().to_string_lossy(),

        "modelsCatalogPath": crate::settings::catalog_path().to_string_lossy(),

        "modelsCatalogSource": crate::settings::catalog_source(),

    })

}



async fn verify_voice_socket(port: u16) -> Result<(), String> {
    VoiceWsSession::connect(port, None).await.map(|_| ())
}

#[allow(dead_code)]
fn _rx_hint(_: Receiver<DictationCommand>) {}


