use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_WS_PORT: u16 = 6016;
pub const DEFAULT_API_PORT: u16 = 6020;
pub const DEFAULT_HOTKEY: &str = "F9";
pub const WS_SUBPROTOCOL: &str = "voxtype-voice-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub hotkey: String,
    pub active_model_id: Option<String>,
    pub runtime_ws_port: u16,
    pub api_port: u16,
    pub strip_trailing_punctuation: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            active_model_id: Some("sensevoice-int8".to_string()),
            runtime_ws_port: DEFAULT_WS_PORT,
            api_port: DEFAULT_API_PORT,
            strip_trailing_punctuation: true,
        }
    }
}

pub fn data_root() -> PathBuf {
    if let Ok(custom) = std::env::var("VOXTYPE_DATA_ROOT") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom);
        }
    }
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("VoxType")
}

pub fn settings_path() -> PathBuf {
    data_root().join("settings.json")
}

pub fn models_dir() -> PathBuf {
    data_root().join("models")
}

pub fn model_dir_for_id(model_id: &str, layout: &str) -> PathBuf {
    models_dir().join(layout)
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if !path.exists() {
        let s = AppSettings::default();
        let _ = save_settings(&s);
        return s;
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let root = data_root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), raw).map_err(|e| e.to_string())
}

pub fn catalog_path() -> PathBuf {
    if let Ok(p) = std::env::var("VOXTYPE_MODELS_CATALOG") {
        return PathBuf::from(p);
    }
    // User override (edit without reinstall)
    let user_catalog = data_root().join("catalog").join("models.json");
    if user_catalog.exists() {
        return user_catalog;
    }
    // Release: bundled next to exe
    if let Some(exe) = std::env::current_exe().ok() {
        if let Some(parent) = exe.parent() {
            let bundled = parent.join("catalog").join("models.json");
            if bundled.exists() {
                return bundled;
            }
        }
    }
    // Dev: repo catalog
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("catalog")
        .join("models.json")
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadSpec {
    /// Primary download URL (use domestic mirror in catalog).
    pub url: String,
    #[serde(default)]
    pub mirror_url: Option<String>,
    #[serde(default)]
    pub fallback_urls: Vec<String>,
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

impl ModelDownloadSpec {
    /// Ordered URLs: mirror → primary → fallbacks (deduped).
    pub fn candidate_urls(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(mirror) = &self.mirror_url {
            push_unique(&mut out, mirror);
        }
        push_unique(&mut out, &self.url);
        for u in &self.fallback_urls {
            push_unique(&mut out, u);
        }
        out
    }
}

fn push_unique(out: &mut Vec<String>, url: &str) {
    let t = url.trim();
    if t.is_empty() {
        return;
    }
    if !out.iter().any(|x| x == t) {
        out.push(t.to_string());
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub default: bool,
    #[serde(rename = "type")]
    pub model_type: String,
    pub layout: String,
    pub download: ModelDownloadSpec,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelsCatalog {
    pub models: Vec<ModelCatalogEntry>,
}

pub fn load_catalog() -> Result<ModelsCatalog, String> {
    let path = catalog_path();
    let raw = fs::read_to_string(&path).map_err(|e| format!("catalog read failed ({path:?}): {e}"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

pub fn runtime_exe_path() -> PathBuf {
    if let Ok(p) = std::env::var("VOXTYPE_RUNTIME_EXE") {
        return PathBuf::from(p);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("runtime")
        .join("dist")
        .join("voxtype-runtime")
        .join("voxtype-runtime.exe");
    if dev.exists() {
        return dev;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("runtime").join("voxtype-runtime.exe")))
        .unwrap_or(dev)
}
