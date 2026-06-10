use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_WS_PORT: u16 = 6016;
pub const DEFAULT_API_PORT: u16 = 6020;
pub const DEFAULT_HOTKEY: &str = "F9";
pub const DEFAULT_HOTKEY_MODE: &str = "auto";
pub const WS_SUBPROTOCOL: &str = "voxtype-voice-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub hotkey: String,
    /// "auto" = short tap toggles, long hold release-to-type; "hold" / "toggle" = fixed behavior
    #[serde(default = "default_hotkey_mode")]
    pub hotkey_mode: String,
    pub active_model_id: Option<String>,
    pub runtime_ws_port: u16,
    pub api_port: u16,
    pub strip_trailing_punctuation: bool,
    /// When true, runtime uses the platform GPU provider (CUDA/CoreML) with CPU fallback.
    #[serde(default = "default_true")]
    pub use_gpu: bool,
}

fn default_hotkey_mode() -> String {
    DEFAULT_HOTKEY_MODE.to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            hotkey_mode: DEFAULT_HOTKEY_MODE.to_string(),
            active_model_id: Some("sensevoice-int8".to_string()),
            runtime_ws_port: DEFAULT_WS_PORT,
            api_port: DEFAULT_API_PORT,
            strip_trailing_punctuation: true,
            use_gpu: true,
        }
    }
}

/// ONNX execution provider when GPU acceleration is enabled (platform-specific).
/// Matches QuickerAgent voice-asr-runtime: DirectML on Windows, CoreML on macOS, CUDA on Linux.
pub fn preferred_gpu_provider() -> &'static str {
    if cfg!(target_os = "macos") {
        "coreml"
    } else if cfg!(windows) {
        "directml"
    } else {
        "cuda"
    }
}

pub fn resolve_runtime_provider(use_gpu: bool) -> &'static str {
    if use_gpu {
        preferred_gpu_provider()
    } else {
        "cpu"
    }
}

pub fn runtime_log_path() -> PathBuf {
    data_root().join("logs").join("runtime.log")
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

pub fn model_dir_for_id(_model_id: &str, layout: &str) -> PathBuf {
    models_dir().join(layout)
}

/// Runtime WS and HTTP API must not share a port — otherwise the app panics on API bind.
fn normalize_listen_ports(settings: &mut AppSettings) {
    if settings.api_port != settings.runtime_ws_port {
        return;
    }
    let mut candidate = DEFAULT_API_PORT;
    if candidate == settings.runtime_ws_port {
        candidate = settings
            .runtime_ws_port
            .saturating_add(1)
            .clamp(RUNTIME_PORT_MIN, RUNTIME_PORT_MAX);
    }
    tracing::warn!(
        "api_port and runtime_ws_port both {}; moving api_port to {candidate}",
        settings.runtime_ws_port
    );
    settings.api_port = candidate;
}

const RUNTIME_PORT_MIN: u16 = 6016;
const RUNTIME_PORT_MAX: u16 = 6100;

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if !path.exists() {
        let s = AppSettings::default();
        let _ = save_settings(&s);
        return s;
    }
    let mut settings = match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    };
    let before_api = settings.api_port;
    normalize_listen_ports(&mut settings);
    if settings.api_port != before_api {
        let _ = save_settings(&settings);
    }
    settings
}

pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let root = data_root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), raw).map_err(|e| e.to_string())
}

// Installed NSIS layout: <install-dir>/catalog/models.json (bundled at build time).
pub fn bundled_catalog_path() -> Option<PathBuf> {
    install_dir().map(|parent| parent.join("catalog").join("models.json"))
}

pub fn catalog_path() -> PathBuf {
    if let Ok(p) = std::env::var("VOXTYPE_MODELS_CATALOG") {
        return PathBuf::from(p);
    }
    // User override for advanced tuning; fresh installs use the bundled catalog.
    let user_catalog = data_root().join("catalog").join("models.json");
    if user_catalog.exists() {
        return user_catalog;
    }
    if let Some(bundled) = bundled_catalog_path() {
        if bundled.is_file() {
            return bundled;
        }
    }
    // Dev: staged copy next to tauri crate, then repo canonical catalog.
    let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundle-resources")
        .join("catalog")
        .join("models.json");
    if staged.is_file() {
        return staged;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("catalog")
        .join("models.json")
}

pub fn catalog_source() -> &'static str {
    if std::env::var("VOXTYPE_MODELS_CATALOG").is_ok() {
        return "env";
    }
    let user_catalog = data_root().join("catalog").join("models.json");
    if user_catalog.exists() {
        return "user-override";
    }
    if bundled_catalog_path().is_some_and(|p| p.is_file()) {
        return "bundled";
    }
    let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundle-resources")
        .join("catalog")
        .join("models.json");
    if staged.is_file() {
        return "dev-staged";
    }
    "dev-repo"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScopeFileSpec {
    pub name: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadSpec {
    #[serde(default = "default_source_zip")]
    pub source: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub mirror_url: Option<String>,
    #[serde(default)]
    pub fallback_urls: Vec<String>,
    #[serde(default)]
    pub modelscope_resolve_base: Option<String>,
    #[serde(default)]
    pub modelscope_files: Vec<ModelScopeFileSpec>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub size_bytes: Option<u64>,
}

fn default_source_zip() -> String {
    "zip".to_string()
}

impl ModelDownloadSpec {
    pub fn is_modelscope(&self) -> bool {
        self.source.eq_ignore_ascii_case("modelscope")
    }

    pub fn is_archive(&self) -> bool {
        self.source.eq_ignore_ascii_case("archive")
            || self.source.eq_ignore_ascii_case("github")
    }

    pub fn candidate_zip_urls(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        if let Some(mirror) = &self.mirror_url {
            push_unique(&mut out, mirror);
        }
        if let Some(url) = &self.url {
            push_unique(&mut out, url);
        }
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
    #[serde(default)]
    pub runtime_preset: Option<String>,
    #[serde(default)]
    pub engine_backend: Option<String>,
    #[serde(default = "default_true")]
    pub supported: bool,
    pub download: ModelDownloadSpec,
}

impl ModelCatalogEntry {
    pub fn runtime_preset_or_type(&self) -> &str {
        self.runtime_preset
            .as_deref()
            .unwrap_or(&self.model_type)
    }

    pub fn prefers_onnx_gguf(&self) -> bool {
        self.engine_backend
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("onnx_gguf"))
            .unwrap_or(matches!(
                self.runtime_preset_or_type(),
                "fun_asr_nano" | "qwen_asr"
            ))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supported: bool,
    pub installed: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelsCatalog {
    #[serde(default)]
    pub schema_version: u32,
    pub models: Vec<ModelCatalogEntry>,
}

pub fn load_catalog() -> Result<ModelsCatalog, String> {
    if std::env::var("VOXTYPE_MODELS_CATALOG").is_ok() {
        return read_catalog_file(&catalog_path());
    }

    let user_path = data_root().join("catalog").join("models.json");
    if user_path.is_file() {
        if let Some(builtin_path) = builtin_catalog_path() {
            let user = read_catalog_file(&user_path)?;
            let builtin = read_catalog_file(&builtin_path)?;
            if user_catalog_is_stale(&user, &builtin) {
                upgrade_user_catalog(&user_path, &builtin)?;
                return Ok(builtin);
            }
        }
        return read_catalog_file(&user_path);
    }

    read_catalog_file(&catalog_path())
}

fn read_catalog_file(path: &Path) -> Result<ModelsCatalog, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("catalog read failed ({path:?}): {e}"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// Built-in catalog shipped with the app (never the user override file).
fn builtin_catalog_path() -> Option<PathBuf> {
    if let Some(bundled) = bundled_catalog_path() {
        if bundled.is_file() {
            return Some(bundled);
        }
    }
    let staged = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bundle-resources")
        .join("catalog")
        .join("models.json");
    if staged.is_file() {
        return Some(staged);
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("catalog")
        .join("models.json");
    if repo.is_file() {
        return Some(repo);
    }
    None
}

fn user_catalog_is_stale(user: &ModelsCatalog, builtin: &ModelsCatalog) -> bool {
    if user.schema_version < builtin.schema_version {
        return true;
    }
    // CapsWriter-era overrides marked hybrid models unsupported while the app supports them.
    builtin.models.iter().any(|built_in| {
        built_in.supported
            && user
                .models
                .iter()
                .any(|entry| entry.id == built_in.id && !entry.supported)
    })
}

fn upgrade_user_catalog(user_path: &Path, builtin: &ModelsCatalog) -> Result<(), String> {
    let backup = user_path.with_extension("json.bak");
    let _ = fs::copy(user_path, &backup);
    let raw = serde_json::to_string_pretty(builtin).map_err(|e| e.to_string())?;
    fs::write(user_path, raw).map_err(|e| e.to_string())?;
    tracing::info!(
        "upgraded stale user model catalog (backup: {})",
        backup.display()
    );
    Ok(())
}

pub fn find_catalog_entry(model_id: &str) -> Result<ModelCatalogEntry, String> {
    let catalog = load_catalog()?;
    catalog
        .models
        .iter()
        .find(|m| m.id == model_id)
        .cloned()
        .ok_or_else(|| format!("未知模型: {model_id}"))
}

fn has_onnx_stem(dir: &Path, stem: &str) -> bool {
    dir.join(format!("{stem}.int8.onnx")).is_file() || dir.join(format!("{stem}.onnx")).is_file()
}

fn has_tokenizer_dir(dir: &Path) -> bool {
    for name in ["tokenizer", "Qwen3-0.6B"] {
        let tok = dir.join(name);
        if tok.is_dir()
            && (tok.join("tokenizer.json").is_file() || tok.join("vocab.json").is_file())
        {
            return true;
        }
    }
    false
}

fn has_paraformer_or_sensevoice_layout(dir: &Path) -> bool {
    let has_tokens = dir.join("tokens.txt").is_file();
    let has_onnx =
        dir.join("model.int8.onnx").is_file() || dir.join("model.onnx").is_file();
    has_tokens && has_onnx
}

fn has_funasr_nano_layout(dir: &Path) -> bool {
    has_onnx_stem(dir, "encoder_adaptor")
        && has_onnx_stem(dir, "llm")
        && has_onnx_stem(dir, "embedding")
        && has_tokenizer_dir(dir)
}

fn has_qwen_asr_layout(dir: &Path) -> bool {
    dir.join("conv_frontend.onnx").is_file()
        && has_onnx_stem(dir, "encoder")
        && has_onnx_stem(dir, "decoder")
        && has_tokenizer_dir(dir)
}

fn has_gguf_decoder(dir: &Path) -> bool {
    dir.read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        })
}

fn has_funasr_hybrid_layout(dir: &Path) -> bool {
    let has_encoder = dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.contains("encoder") && name.ends_with(".onnx")
        });
    let has_ctc = dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.contains("ctc") && name.ends_with(".onnx")
        });
    has_encoder && has_ctc && has_gguf_decoder(dir) && dir.join("tokens.txt").is_file()
}

fn has_qwen_hybrid_layout(dir: &Path) -> bool {
    let has_frontend = dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.contains("encoder_frontend") && name.ends_with(".onnx")
        });
    let has_backend = dir
        .read_dir()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.contains("encoder_backend") && name.ends_with(".onnx")
        });
    has_frontend && has_backend && has_gguf_decoder(dir)
}

fn has_whisper_layout(dir: &Path) -> bool {
    let has_tokens = dir.join("tokens.txt").is_file();
    let has_encoder = dir.join("encoder.int8.onnx").is_file()
        || dir.join("encoder.onnx").is_file();
    let has_decoder = dir.join("decoder.int8.onnx").is_file()
        || dir.join("decoder.onnx").is_file();
    has_tokens && has_encoder && has_decoder
}

pub fn is_model_installed(entry: &ModelCatalogEntry) -> bool {
    let dir = model_dir_for_id(&entry.id, &entry.layout);
    is_model_installed_at(&dir, entry)
}

pub fn is_model_installed_at(dir: &Path, entry: &ModelCatalogEntry) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let kind = entry.runtime_preset_or_type();
    match kind {
        "whisper" => has_whisper_layout(dir),
        "fun_asr_nano" => {
            if entry.prefers_onnx_gguf() {
                has_funasr_hybrid_layout(dir) || has_funasr_nano_layout(dir)
            } else {
                has_funasr_nano_layout(dir)
            }
        }
        "qwen_asr" => {
            if entry.prefers_onnx_gguf() {
                has_qwen_hybrid_layout(dir) || has_qwen_asr_layout(dir)
            } else {
                has_qwen_asr_layout(dir)
            }
        }
        _ => has_paraformer_or_sensevoice_layout(dir),
    }
}

/// Resolve the model to run, falling back to an installed model when the saved choice is missing.
pub fn resolve_active_model(settings: &AppSettings) -> Result<ModelCatalogEntry, String> {
    let catalog = load_catalog()?;
    let preferred_id = settings
        .active_model_id
        .as_deref()
        .or_else(|| catalog.models.iter().find(|m| m.default).map(|m| m.id.as_str()))
        .ok_or_else(|| "未配置模型".to_string())?;

    if let Some(entry) = catalog.models.iter().find(|m| m.id == preferred_id) {
        if entry.supported && is_model_installed(entry) {
            return Ok(entry.clone());
        }
    }

    let mut candidates: Vec<&ModelCatalogEntry> = catalog
        .models
        .iter()
        .filter(|m| m.supported && is_model_installed(m))
        .collect();
    candidates.sort_by_key(|m| (!m.default, m.id.clone()));

    let Some(fallback) = candidates.first() else {
        return Err("未找到已安装的识别模型，请先在设置中下载模型".into());
    };

    if settings.active_model_id.as_deref() != Some(fallback.id.as_str()) {
        let mut next = settings.clone();
        next.active_model_id = Some(fallback.id.clone());
        save_settings(&next)?;
        tracing::warn!(
            "active model {:?} unavailable; fell back to {}",
            settings.active_model_id,
            fallback.name
        );
    }
    Ok((*fallback).clone())
}

pub fn list_model_statuses() -> Result<Vec<ModelStatusDto>, String> {
    let catalog = load_catalog()?;
    let settings = load_settings();
    let active = settings.active_model_id.as_deref();
    Ok(catalog
        .models
        .iter()
        .map(|m| ModelStatusDto {
            id: m.id.clone(),
            name: m.name.clone(),
            description: m.description.clone(),
            supported: m.supported,
            installed: is_model_installed(m),
            active: active == Some(m.id.as_str()),
        })
        .collect())
}

fn install_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
}

fn runtime_exe_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(p) = std::env::var("VOXTYPE_RUNTIME_EXE") {
        paths.push(PathBuf::from(p));
    }
    paths.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("runtime")
            .join("dist")
            .join("voxtype-runtime")
            .join("voxtype-runtime.exe"),
    );
    if let Some(parent) = install_dir() {
        paths.push(
            parent
                .join("runtime")
                .join("voxtype-runtime")
                .join("voxtype-runtime.exe"),
        );
    }
    paths
}

pub fn runtime_exe_path() -> PathBuf {
    runtime_exe_candidates()
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            runtime_exe_candidates()
                .into_iter()
                .next()
                .unwrap_or_else(|| PathBuf::from("voxtype-runtime.exe"))
        })
}

#[allow(dead_code)]
pub fn model_layout_path(layout: &str) -> PathBuf {
    models_dir().join(layout)
}

#[allow(dead_code)]
pub fn tokens_exists(dir: &Path) -> bool {
    dir.join("tokens.txt").is_file()
}
