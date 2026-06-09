use crate::settings::{load_catalog, load_settings, model_dir_for_id, models_dir, save_settings};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{copy, Read, Write};
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

    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({ "percent": 5 }),
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&entry.download.url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("下载 HTTP {}", response.status()));
    }

    let total = response.content_length();
    let mut reader = response.bytes_stream();
    use futures_util::StreamExt;
    let mut file = File::create(&zip_path).map_err(|e| e.to_string())?;
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
    drop(file);

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

fn verify_sha256(path: &std::path::Path, expected_hex: &str) -> Result<(), String> {
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

fn extract_zip(zip_path: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let out_path = dest.join(name.trim_start_matches('/'));
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
