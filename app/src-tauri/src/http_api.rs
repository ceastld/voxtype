use crate::dictation::DictationHandle;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub struct ApiState {
    pub dictation: Arc<DictationHandle>,
}

pub fn router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/dictate/start", post(dictate_start))
        .route("/dictate/stop", post(dictate_stop))
        .route("/dictate/toggle", post(dictate_toggle))
        .route("/status", get(status))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true, "service": "voxtype" }))
}

async fn status(State(state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    Json(crate::dictation::build_status(&state.dictation).await)
}

async fn dictate_start(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .dictation
        .start_recording()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn dictate_stop(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let text = state
        .dictation
        .stop_recording_and_type()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({ "ok": true, "text": text })))
}

async fn dictate_toggle(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .dictation
        .toggle()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn serve(state: Arc<ApiState>, port: u16) {
    let app = router(state);
    let addr = format!("127.0.0.1:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("VoxType API failed to bind {addr}: {e}");
            return;
        }
    };
    tracing::info!("VoxType API listening on http://{addr}");
    axum::serve(listener, app).await.ok();
}
