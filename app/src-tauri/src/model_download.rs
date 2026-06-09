use crate::settings::{
    find_catalog_entry, is_model_installed, load_settings, model_dir_for_id,
    models_dir, save_settings, ModelCatalogEntry, ModelScopeFileSpec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{copy, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartMeta {
    url: String,
    total: Option<u64>,
    downloaded: u64,
}

pub async fn download_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    let entry = find_catalog_entry(model_id)?;
    if !entry.supported {
        return Err(format!(
            "「{}」尚未支持",
            entry.name
        ));
    }

    fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    let dest = model_dir_for_id(&entry.id, &entry.layout);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;

    emit_progress(
        app,
        &entry,
        2,
        "连接 ModelScope…",
        false,
    );

    if entry.download.is_modelscope() {
        download_from_modelscope(app, &entry, &dest).await?;
    } else if entry.download.is_archive() {
        download_from_archive(app, &entry, &dest).await?;
    } else {
        download_from_zip(app, &entry, &dest).await?;
    }

    if !is_model_installed(&entry) {
        return Err("模型文件不完整，请重试下载".into());
    }

    emit_progress(app, &entry, 100, "校验完成", true);
    let _ = app.emit("model-download-done", ());

    let mut settings = load_settings();
    settings.active_model_id = Some(entry.id.clone());
    save_settings(&settings)?;

    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    percent: u8,
    message: &str,
    done: bool,
) {
    let _ = app.emit(
        "model-download-progress",
        serde_json::json!({
            "percent": percent,
            "message": message,
            "modelId": entry.id,
            "modelName": entry.name,
            "done": done,
        }),
    );
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
        match download_file_resumable(app, entry, &url, &out_path, spec, done_size, total_size)
            .await
        {
            Ok(bytes) => {
                if let Some(expected) = &spec.sha256 {
                    verify_file_sha256(&out_path, expected)?;
                }
                done_size += spec.size.unwrap_or(bytes);
            }
            Err(e) if !spec.required => {
                tracing::warn!("optional model file skipped {}: {e}", spec.name);
            }
            Err(e) => return Err(format!("下载 {} 失败: {e}", spec.name)),
        }
    }

    Ok(())
}

async fn download_file_resumable(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    url: &str,
    out_path: &Path,
    spec: &ModelScopeFileSpec,
    done_base: u64,
    total_all: u64,
) -> Result<u64, String> {
    emit_progress(
        app,
        entry,
        file_percent(done_base, total_all, 0, spec.size),
        &format!("下载 {}", spec.name),
        false,
    );
    download_url_resumable(app, entry, url, out_path, spec.size, done_base, total_all).await?;
    let len = fs::metadata(out_path).map_err(|e| e.to_string())?.len();
    Ok(len)
}

fn file_percent(done_base: u64, total_all: u64, file_done: u64, file_total: Option<u64>) -> u8 {
    if total_all == 0 {
        return 5;
    }
    let file_total = file_total.unwrap_or(0);
    let current = done_base.saturating_add(file_done.min(file_total));
    (5 + ((current * 90) / total_all.max(1)) as u8).min(95)
}

fn part_paths(out_path: &Path) -> (PathBuf, PathBuf) {
    let name = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let part = out_path.with_file_name(format!("{name}.part"));
    let meta = out_path.with_file_name(format!("{name}.part.meta.json"));
    (part, meta)
}

async fn download_url_resumable(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    url: &str,
    out_path: &Path,
    known_total: Option<u64>,
    done_base: u64,
    total_all: u64,
) -> Result<(), String> {
    let (part_path, meta_path) = part_paths(out_path);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut resume_from = 0u64;
    if part_path.is_file() {
        if let Ok(meta_raw) = fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<PartMeta>(&meta_raw) {
                if meta.url == url {
                    resume_from = meta.downloaded;
                }
            }
        }
        if resume_from == 0 {
            resume_from = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .user_agent("VoxType/0.1 (ModelScope downloader)")
        .build()
        .map_err(|e| e.to_string())?;

    let mut request = client.get(url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
        emit_progress(
            app,
            entry,
            file_percent(done_base, total_all, resume_from, known_total),
            "断点续传…",
            false,
        );
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("连接失败: {e}"))?;

    let status = response.status();
    if !status.is_success() && status.as_u16() != 206 {
        if resume_from > 0 && status.as_u16() == 416 {
            fs::rename(&part_path, out_path).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&meta_path);
            return Ok(());
        }
        return Err(format!("HTTP {}", status));
    }

    if resume_from > 0 && status.as_u16() == 200 {
        resume_from = 0;
        let _ = fs::remove_file(&part_path);
    }

    let total = response
        .content_length()
        .map(|n| n + resume_from)
        .or(known_total.map(|t| t))
        .or_else(|| {
            response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split('/').nth(1))
                .and_then(|s| s.parse().ok())
        });

    let mut reader = response.bytes_stream();
    use futures_util::StreamExt;

    let mut file = OpenOptions::new()
        .create(true)
        .append(resume_from > 0)
        .truncate(resume_from == 0)
        .write(true)
        .open(&part_path)
        .map_err(|e| e.to_string())?;

    if resume_from > 0 {
        file.seek(SeekFrom::Start(resume_from))
            .map_err(|e| e.to_string())?;
    }

    let mut downloaded = resume_from;
    while let Some(chunk) = reader.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let meta = PartMeta {
            url: url.to_string(),
            total,
            downloaded,
        };
        let _ = fs::write(&meta_path, serde_json::to_string(&meta).unwrap_or_default());

        if total_all > 0 || total.is_some() {
            let file_done = downloaded.saturating_sub(resume_from);
            emit_progress(
                app,
                entry,
                file_percent(done_base, total_all, file_done, known_total.or(total)),
                &out_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("下载中"),
                false,
            );
        }
    }

    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    if let Some(expected) = known_total {
        if downloaded < expected {
            return Err(format!("下载不完整 ({downloaded}/{expected} bytes)"));
        }
    }

    fs::rename(&part_path, out_path).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&meta_path);
    Ok(())
}

async fn download_from_archive(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    dest: &Path,
) -> Result<(), String> {
    let archive_path = models_dir().join(format!("{}.tar.bz2", entry.id));
    let (part_path, meta_path) = part_paths(&archive_path);
    let urls = entry.download.candidate_zip_urls();
    if urls.is_empty() {
        return Err("模型配置缺少下载地址".into());
    }

    let mut last_err: Option<String> = None;
    let mut ok = false;
    for url in &urls {
        match download_url_resumable(
            app,
            entry,
            url,
            &archive_path,
            entry.download.size_bytes,
            0,
            1,
        )
        .await
        {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => {
                last_err = Some(e);
                let _ = fs::remove_file(&part_path);
                let _ = fs::remove_file(&meta_path);
                let _ = fs::remove_file(&archive_path);
            }
        }
    }
    if !ok {
        return Err(last_err.unwrap_or_else(|| "所有下载源均失败".into()));
    }

    if let Some(expected) = &entry.download.sha256 {
        verify_sha256_file(&archive_path, expected)?;
    }
    extract_tar_bz2(&archive_path, dest)?;
    let _ = fs::remove_file(&archive_path);
    Ok(())
}

async fn download_from_zip(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    dest: &Path,
) -> Result<(), String> {
    let zip_path = models_dir().join(format!("{}.zip", entry.id));
    let (part_path, meta_path) = part_paths(&zip_path);
    let urls = entry.download.candidate_zip_urls();
    if urls.is_empty() {
        return Err("模型配置缺少下载地址".into());
    }

    let mut last_err: Option<String> = None;
    let mut ok = false;
    for url in &urls {
        match download_url_resumable(app, entry, url, &zip_path, entry.download.size_bytes, 0, 1)
            .await
        {
            Ok(()) => {
                ok = true;
                break;
            }
            Err(e) => {
                last_err = Some(e);
                let _ = fs::remove_file(&part_path);
                let _ = fs::remove_file(&meta_path);
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
        Err(format!("SHA256 校验失败: {digest} != {expected_hex}"))
    }
}

fn verify_file_sha256(path: &Path, expected_hex: &str) -> Result<(), String> {
    verify_sha256_file(path, expected_hex)
}

fn extract_tar_bz2(archive_path: &Path, dest: &Path) -> Result<(), String> {
    use bzip2::read::BzDecoder;
    use tar::Archive;

    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }

    let temp = std::env::temp_dir().join(format!("voxtype-extract-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp).map_err(|e| e.to_string())?;

    let file = File::open(archive_path).map_err(|e| e.to_string())?;
    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive.unpack(&temp).map_err(|e| e.to_string())?;

    let mut children: Vec<PathBuf> = fs::read_dir(&temp)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|d| d.path()))
        .collect();
    let source = if children.len() == 1 && children[0].is_dir() {
        children.remove(0)
    } else {
        temp.clone()
    };

    copy_dir_recursive(&source, dest)?;
    let test_wavs = dest.join("test_wavs");
    if test_wavs.is_dir() {
        let _ = fs::remove_dir_all(&test_wavs);
    }
    let _ = fs::remove_dir_all(&temp);
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
