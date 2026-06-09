use crate::settings::{load_catalog, load_settings, model_dir_for_id, models_dir, save_settings};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

pub async fn download_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let catalog = load_catalog()?;
    let entry = catalog
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("未知模型: {model_id}"))?;

    fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    let zip_path = models_dir().join(format!("{model_id}.zip"));

    let urls = entry.download.candidate_urls();
    if urls.is_empty() {
        return Err("模型配置缺少下载地址".into());
    }

    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "percent": 5 }),
    );

    let mut last_err: Option<String> = None;
    let mut downloaded = false;
    for (idx, url) in urls.iter().enumerate() {
        tracing::info!("model download try {}/{}: {}", idx + 1, urls.len(), url);
        match download_zip_from_url(app, url, &zip_path).await {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => {
                tracing::warn!("model download failed ({url}): {e}");
                last_err = Some(e);
                let _ = fs::remove_file(&zip_path);
            }
        }
    }

    if !downloaded {
        return Err(last_err.unwrap_or_else(|| "所有下载源均失败".into()));
    }

    if let Some(expected) = &entry.download.sha256 {
        verify_sha256(&zip_path, expected)?;
    }

    let dest = model_dir_for_id(&entry.id, &entry.layout);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    extract_zip(&zip_path, &dest)?;

    let _ = fs::remove_file(&zip_path);
    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "percent": 100 }),
    );
    let _ = app.emit("model-download-done", ());

    let mut settings = load_settings();
    settings.active_model_id = Some(entry.id.clone());
    save_settings(&settings)?;

    Ok(())
}

async fn download_zip_from_url(
    app: &AppHandle,
    url: &str,
    zip_path: &Path,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
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

    let total = response.content_length();
    let mut reader = response.bytes_stream();
    use futures_util::StreamExt;
    let mut file = File::create(zip_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;

    while let Some(chunk) = reader.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total {
            let pct = ((downloaded * 90) / total.max(1)) as u8 + 5;
            let _ = app.emit(
                "model-download-progress",
                serde_json::json!({ "percent": pct.min(95) }),
            );
        }
    }

    Ok(())
}

pub fn activate_model(model_id: &str) -> Result<(), String> {
    let catalog = load_catalog()?;
    let entry = catalog
        .models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("未知模型: {model_id}"))?;
    let dir = model_dir_for_id(&entry.id, &entry.layout);
    if !dir.exists() {
        return Err("请先下载该模型".into());
    }
    let mut settings = load_settings();
    settings.active_model_id = Some(model_id.to_string());
    save_settings(&settings)
}

fn verify_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
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
