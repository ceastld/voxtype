use crate::overlay;
use crate::settings::WS_SUBPROTOCOL;
use futures_util::{SinkExt, StreamExt};
use tauri::AppHandle;

use serde_json::json;

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot, Mutex};

use tokio_tungstenite::{

    connect_async,

    tungstenite::{client::IntoClientRequest, Message},

};



#[derive(Debug, Clone)]

pub struct TranscribeResult {

    pub text: String,

}



#[derive(Debug, Clone, serde::Serialize)]

#[serde(rename_all = "camelCase")]

pub struct RuntimeHealth {

    pub ok: bool,

    pub ready: bool,

    pub model_loaded: bool,

    pub model_id: Option<String>,

    pub detail: Option<String>,

    pub execution_provider: Option<String>,

}



struct SessionInner {

    session_id: String,

    ws_tx: mpsc::UnboundedSender<Message>,

    final_tx: Mutex<Option<oneshot::Sender<Result<TranscribeResult, String>>>>,

    started: Mutex<bool>,

    start_error: Mutex<Option<String>>,

    app: Option<AppHandle>,

}



#[derive(Clone)]

pub struct VoiceWsSession {

    inner: Arc<SessionInner>,

}



impl VoiceWsSession {

    pub async fn connect(port: u16, app: Option<AppHandle>) -> Result<Self, String> {

        let url = format!("ws://127.0.0.1:{port}");

        let mut request = url

            .into_client_request()

            .map_err(|e| e.to_string())?;

        request

            .headers_mut()

            .insert("Sec-WebSocket-Protocol", WS_SUBPROTOCOL.parse().unwrap());



        let (ws, _) = connect_async(request)

            .await

            .map_err(|e| format!("无法连接语音服务 (ws://127.0.0.1:{port}): {e}"))?;



        let (mut write, mut read) = ws.split();

        let session_id = uuid::Uuid::new_v4().to_string();

        let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<Message>();

        let final_tx: Mutex<Option<oneshot::Sender<Result<TranscribeResult, String>>>> =

            Mutex::new(None);

        let started = Mutex::new(false);

        let start_error = Mutex::new(None);



        let inner = Arc::new(SessionInner {

            session_id: session_id.clone(),

            ws_tx,

            final_tx,

            started,

            start_error,

            app,

        });



        tokio::spawn(async move {

            while let Some(msg) = ws_rx.recv().await {

                if write.send(msg).await.is_err() {

                    break;

                }

            }

        });



        let reader_inner = Arc::clone(&inner);

        tokio::spawn(async move {

            while let Some(Ok(msg)) = read.next().await {

                if let Message::Text(raw) = msg {

                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {

                        continue;

                    };

                    let msg_sid = v.get("sessionId").and_then(|x| x.as_str());

                    if let Some(s) = msg_sid {

                        if s != reader_inner.session_id {

                            continue;

                        }

                    }

                    match v.get("type").and_then(|x| x.as_str()) {

                        Some("session.started") => {

                            *reader_inner.started.lock().await = true;

                        }

                        Some("partial") => {

                            let text = v

                                .get("text")

                                .and_then(|x| x.as_str())

                                .unwrap_or("")

                                .to_string();

                            if let Some(app) = reader_inner.app.as_ref() {

                                overlay::emit_partial_text(app, &text);

                            }

                        }

                        Some("final") => {

                            let text = v

                                .get("text")

                                .and_then(|x| x.as_str())

                                .unwrap_or("")

                                .to_string();

                            if let Some(app) = reader_inner.app.as_ref() {

                                overlay::emit_partial_text(app, &text);

                            }

                            if let Some(tx) = reader_inner.final_tx.lock().await.take() {

                                let _ = tx.send(Ok(TranscribeResult { text }));

                            }

                        }

                        Some("error") => {

                            let message = v

                                .get("message")

                                .or_else(|| v.get("code"))

                                .and_then(|x| x.as_str())

                                .unwrap_or("语音识别失败")

                                .to_string();

                            if !*reader_inner.started.lock().await {

                                *reader_inner.start_error.lock().await = Some(message.clone());

                            }

                            if let Some(tx) = reader_inner.final_tx.lock().await.take() {

                                let _ = tx.send(Err(message));

                            }

                        }

                        _ => {}

                    }

                }

            }

        });



        Ok(Self { inner })

    }



    pub async fn start_session(&self) -> Result<(), String> {

        let payload = json!({

            "type": "session.start",

            "sessionId": self.inner.session_id,

            "language": "zh-CN",

            "streaming": true,

            "sampleRate": 16000,

            "channels": 1,

            "encoding": "pcm_s16le",

        });

        self.inner

            .ws_tx

            .send(Message::Text(payload.to_string()))

            .map_err(|e| e.to_string())?;



        for _ in 0..80 {

            if let Some(err) = self.inner.start_error.lock().await.clone() {

                return Err(err);

            }

            if *self.inner.started.lock().await {

                return Ok(());

            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        }

        if let Some(err) = self.inner.start_error.lock().await.clone() {

            return Err(err);

        }

        Err("语音服务响应超时，请确认模型已加载完成".into())

    }



    pub fn send_pcm(&self, pcm: &[u8]) {

        if pcm.is_empty() {

            return;

        }

        let _ = self.inner.ws_tx.send(Message::Binary(pcm.to_vec()));

    }



    pub async fn end_session(&self) -> Result<TranscribeResult, String> {

        let (tx, rx) = oneshot::channel();

        *self.inner.final_tx.lock().await = Some(tx);



        let payload = json!({

            "type": "session.end",

            "sessionId": self.inner.session_id,

        });

        self.inner

            .ws_tx

            .send(Message::Text(payload.to_string()))

            .map_err(|e| e.to_string())?;



        tokio::time::timeout(std::time::Duration::from_secs(60), rx)

            .await

            .map_err(|_| "识别超时（60s）".to_string())?

            .map_err(|_| "识别通道关闭".to_string())?

    }

}



pub async fn fetch_runtime_health(port: u16) -> Option<RuntimeHealth> {

    let url = format!("http://127.0.0.1:{port}/health");

    let resp = reqwest::get(url).await.ok()?;

    let v = resp.json::<serde_json::Value>().await.ok()?;

    Some(RuntimeHealth {

        ok: v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false),

        ready: v.get("ready").and_then(|x| x.as_bool()).unwrap_or(false),

        model_loaded: v.get("modelLoaded").and_then(|x| x.as_bool()).unwrap_or(false),

        model_id: v

            .get("modelId")

            .and_then(|x| x.as_str())

            .map(|s| s.to_string()),

        detail: v

            .get("message")

            .and_then(|x| x.as_str())

            .map(|s| s.to_string()),

        execution_provider: v

            .get("executionProvider")

            .and_then(|x| x.as_str())

            .map(|s| s.to_string()),

    })

}



pub async fn check_runtime_health(port: u16) -> bool {

    fetch_runtime_health(port)

        .await

        .map(|h| h.ok && h.ready)

        .unwrap_or(false)

}


