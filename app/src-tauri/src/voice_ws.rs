use crate::settings::WS_SUBPROTOCOL;
use futures_util::{SinkExt, StreamExt};
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

struct SessionInner {
    session_id: String,
    ws_tx: mpsc::UnboundedSender<Message>,
    final_tx: Mutex<Option<oneshot::Sender<Result<TranscribeResult, String>>>>,
    started: Mutex<bool>,
}

#[derive(Clone)]
pub struct VoiceWsSession {
    inner: Arc<SessionInner>,
}

impl VoiceWsSession {
    pub async fn connect(port: u16) -> Result<Self, String> {
        let url = format!("ws://127.0.0.1:{port}");
        let mut request = url
            .into_client_request()
            .map_err(|e| e.to_string())?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", WS_SUBPROTOCOL.parse().unwrap());

        let (ws, _) = connect_async(request)
            .await
            .map_err(|e| format!("无法连接语音服务: {e}"))?;

        let (mut write, mut read) = ws.split();
        let session_id = uuid::Uuid::new_v4().to_string();
        let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<Message>();
        let final_tx: Mutex<Option<oneshot::Sender<Result<TranscribeResult, String>>>> =
            Mutex::new(None);
        let started = Mutex::new(false);

        let inner = Arc::new(SessionInner {
            session_id: session_id.clone(),
            ws_tx,
            final_tx,
            started,
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
                        Some("final") => {
                            let text = v
                                .get("text")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
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

        for _ in 0..40 {
            if *self.inner.started.lock().await {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Err("语音服务响应超时".into())
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

        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| "识别超时".to_string())?
            .map_err(|_| "识别通道关闭".to_string())?
    }
}

pub async fn check_runtime_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    let Ok(resp) = reqwest::get(url).await else {
        return false;
    };
    let Ok(v) = resp.json::<serde_json::Value>().await else {
        return false;
    };
    v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false)
        && v.get("ready").and_then(|x| x.as_bool()).unwrap_or(false)
}
