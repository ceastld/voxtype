use crate::settings::{
    find_catalog_entry, is_model_installed, load_settings, model_dir_for_id,
    models_dir, save_settings, ModelCatalogEntry, ModelScopeFileSpec,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

pub async fn download_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let entry = find_catalog_entry(model_id)?;
    if !entry.supported {
        return Err(format!(
            "「{}」尚未支持，请先在设置中选择 SenseVoice 或 Paraformer",
            entry.name
        ));
    }

    fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    let dest = model_dir_for_id(&entry.id, &entry.layout);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "percent": 2, "message": "连接 ModelScope…" }),
    );

    if entry.download.is_modelscope() {
        download_from_modelscope(app, &entry, &dest).await?;
    } else {
        download_from_zip(app, &entry, &dest).await?;
    }

    if !is_model_installed(&entry) {
        return Err("模型文件不完整，请重试下载".into());
    }

    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "percent": 100, "message": "完成" }),
    );
    let _ = app.emit("model-download-done", ());

    let mut settings = load_settings();
    settings.active_model_id = Some(entry.id.clone());
    save_settings(&settings)?;

    Ok(())
}

async fn download_from_modelscope(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    dest: &Path,
) -> Result<(), String> {
    let base = entry
        .download
        .modelscope_resolve_base
        .as_deref()
        .ok_or_else(|| format!("「{}」缺少 ModelScope 配置", entry.name))?
        .trim_end_matches('/');

    let files = &entry.download.modelscope_files;
    if files.is_empty() {
        return Err(format!("「{}」未配置 ModelScope 文件列表", entry.name));
    }

    let total_size: u64 = files.iter().filter_map(|f| f.size).sum();
    let mut done_size: u64 = 0;

    for spec in files {
        let url = format!("{base}/{}", spec.name);
        let out_path = dest.join(&spec.name);
        match download_file_with_progress(app, &url, &out_path, spec).await {
            Ok(bytes) => {
                if let Some(expected) = &spec.sha256 {
                    verify_file_sha256(&out_path, expected)?;
                }
                done_size += spec.size.unwrap_or(bytes);
                if total_size > 0 {
                    let pct = 5 + ((done_size * 90) / total_size) as u8;
                    let _ = app.emit(
                        "model-download-progress",
                        serde_json::json!({ "percent": pct.min(95), "message": spec.name }),
                    );
                }
            }
            Err(e) if !spec.required => {
                tracing::warn!("optional model file skipped {}: {e}", spec.name);
            }
            Err(e) => return Err(format!("下载 {} 失败: {e}", spec.name)),
        }
    }

    Ok(())
}

async fn download_from_zip(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    dest: &Path,
) -> Result<(), String> {
    let zip_path = models_dir().join(format!("{}.zip", entry.id));
    let urls = entry.download.candidate_zip_urls();
    if urls.is_empty() {
        return Err("模型配置缺少下载地址".into());
    }

    let mut last_err: Option<String> = None;
    let mut ok = false;
    for url in &urls {
        match download_url_to_file(app, url, &zip_path, None).await {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => {
                last_err = Some(e);
                let _ = fs::remove_file(&zip_path);
            }
        }
    }
    if !ok {
        return Err(last_err.unwrap_or_else(|| "所有下载源均失败".into()));
    }

    if let Some(expected) = &entry.download.sha256 {
        verify_sha256_file(&zip_path, expected)?;
    }
    extract_zip(&zip_path, dest)?;
    let _ = fs::remove_file(&zip_path);
    Ok(())
}

async fn download_file_with_progress(
    app: &AppHandle,
    url: &str,
    out_path: &Path,
    spec: &ModelScopeFileSpec,
) -> Result<u64, String> {
    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "message": format!("下载 {}", spec.name) }),
    );
    download_url_to_file(app, url, out_path, spec.size).await?;
    let len = fs::metadata(out_path).map_err(|e| e.to_string())?.len();
    Ok(len)
}

async fn download_url_to_file(
    app: &AppHandle,
    url: &str,
    out_path: &Path,
    known_total: Option<u64>,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .user_agent("VoxType/0.1 (ModelScope downloader)")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("连接失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let total = response.content_length().or(known_total);
    let mut reader = response.bytes_stream();
    use futures_util::StreamExt;
    let mut file = File::create(out_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut buf = Vec::new();

    while let Some(chunk) = reader.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        buf.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if let Some(total) = total {
            let pct = 5 + ((downloaded * 85) / total.max(1)) as u8;
            let _ = app.emit(
                "model-download-progress",
                serde_json::json!({ "percent": pct.min(95) }),
            );
        }
    }

    Ok(())
}

pub fn activate_model(model_id: &str) -> Result<(), String> {
    let entry = find_catalog_entry(model_id)?;
    if !entry.supported {
        return Err(format!("「{}」尚未支持，无法切换", entry.name));
    }
    if !is_model_installed(&entry) {
        return Err(format!("请先下载「{}」", entry.name));
    }
    let mut settings = load_settings();
    settings.active_model_id = Some(model_id.to_string());
    save_settings(&settings)
}

fn verify_sha256_file(path: &Path, expected_hex: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest.eq_ignore_ascii_case(expected_hex.trim()) {
        Ok(())
    } else {
        Err("SHA256 校验失败".into())
    }
}

fn verify_file_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
    verify_sha256_file(path, expected_hex)
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let out_path: PathBuf = dest.join(name.trim_start_matches('/'));
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = File::create(&out_path).map_err(|e| e.to_string())?;
            copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
