use crate::settings::{
    find_catalog_entry, is_model_installed, is_model_installed_at, load_settings, model_dir_for_id,
    models_dir, save_settings, ModelCatalogEntry, ModelScopeFileSpec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{copy, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartMeta {
    url: String,
    total: Option<u64>,
    downloaded: u64,
}

const CONNECT_TIMEOUT_SECS: u64 = 30;
const READ_STALL_TIMEOUT_SECS: u64 = 120;
const HTTP_PER_URL_RETRIES: u32 = 2;
const CURL_MIN_BYTES: u64 = 10 * 1024 * 1024;

fn clear_part_files(out_path: &Path) {
    let (part_path, meta_path) = part_paths(out_path);
    let _ = fs::remove_file(&part_path);
    let _ = fs::remove_file(&meta_path);
    let _ = fs::remove_file(out_path);
}

fn reset_part_for_url(out_path: &Path, url: &str) {
    let (part_path, meta_path) = part_paths(out_path);
    if !part_path.is_file() {
        return;
    }
    let same_url = fs::read_to_string(&meta_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<PartMeta>(&raw).ok())
        .is_some_and(|meta| meta.url == url);
    if !same_url {
        clear_part_files(out_path);
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(900))
        .user_agent("VoxType/0.1 (model downloader)")
        .build()
        .map_err(|e| e.to_string())
}

async fn download_with_mirrors(
    app: &AppHandle,
    entry: &ModelCatalogEntry,
    urls: Vec<String>,
    out_path: &Path,
    known_total: Option<u64>,
    done_base: u64,
    total_all: u64,
) -> Result<(), String> {
    if urls.is_empty() {
        return Err("模型配置缺少下载地址".into());
    }

    let mut last_err: Option<String> = None;
    let total = urls.len();
    let prefer_curl = cfg!(windows) && known_total.unwrap_or(0) >= CURL_MIN_BYTES;

    for (index, url) in urls.iter().enumerate() {
        reset_part_for_url(out_path, url);
        emit_progress(
            app,
            entry,
            3,
            &format!("尝试下载源 {}/{}…", index + 1, total),
            false,
        );

        if prefer_curl {
            match download_via_curl(app, entry, url, out_path, known_total, done_base, total_all)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!("curl download failed ({url}): {e}");
                    last_err = Some(e);
                    clear_part_files(out_path);
                    continue;
                }
            }
        }

        for attempt in 0..HTTP_PER_URL_RETRIES {
            if attempt > 0 {
                emit_progress(
                    app,
                    entry,
                    4,
                    &format!("重试下载源 {}/{}…", index + 1, total),
                    false,
                );
            }
            match download_url_resumable(
                app,
                entry,
                url,
                out_path,
                known_total,
                done_base,
                total_all,
            )
            .await
            {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(
                        "http download failed ({url}, attempt {}): {e}",
                        attempt + 1
                    );
                    last_err = Some(e.clone());
                    if e.contains("停滞") || e.contains("连接") || e.contains("HTTP") {
                        clear_part_files(out_path);
                    }
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "所有下载源均失败，请检查网络或稍后重试".into()))
}

#[cfg(windows)]
async fn download_via_curl(
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

    let mut cmd = tokio::process::Command::new("curl.exe");
    cmd.args([
        "-L",
        "--fail",
        "--retry",
        "3",
        "--retry-delay",
        "2",
        "--connect-timeout",
        "30",
        "--max-time",
        "7200",
        "-C",
        "-",
        "-o",
    ]);
    cmd.arg(&part_path);
    cmd.arg(url);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("无法启动 curl.exe: {e}"))?;

    let label = out_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("模型包");

    let started = std::time::Instant::now();
    let mut last_downloaded = 0u64;

    emit_progress(
        app,
        entry,
        connecting_percent(0),
        &format!("{label}… 正在建立连接"),
        false,
    );

    loop {
        tokio::select! {
            status = child.wait() => {
                let status = status.map_err(|e| e.to_string())?;
                if !status.success() {
                    return Err(format!(
                        "curl 下载失败 (exit={:?})",
                        status.code()
                    ));
                }
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let downloaded = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
                let meta = PartMeta {
                    url: url.to_string(),
                    total: known_total,
                    downloaded,
                };
                let _ = fs::write(&meta_path, serde_json::to_string(&meta).unwrap_or_default());

                let elapsed = started.elapsed().as_secs();
                let (percent, message) = if downloaded == 0 {
                    (
                        connecting_percent(elapsed),
                        format!("{label}… 连接中 ({elapsed}s)"),
                    )
                } else {
                    let mb = downloaded / (1024 * 1024);
                    let msg = if let Some(total) = known_total {
                        format!("{label}… {mb} / {} MB", total / (1024 * 1024))
                    } else {
                        format!("{label}… {mb} MB")
                    };
                    let msg = if downloaded == last_downloaded {
                        format!("{msg} · 等待数据")
                    } else {
                        msg
                    };
                    (
                        file_percent(done_base, total_all, downloaded, known_total),
                        msg,
                    )
                };
                last_downloaded = downloaded;
                emit_progress(app, entry, percent, &message, false);
            }
        }
    }

    if let Some(expected) = known_total {
        let downloaded = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        if downloaded + 1024 < expected {
            return Err(format!("curl 下载不完整 ({downloaded}/{expected} bytes)"));
        }
    }

    fs::rename(&part_path, out_path).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&meta_path);
    Ok(())
}

#[cfg(not(windows))]
async fn download_via_curl(
    _app: &AppHandle,
    _entry: &ModelCatalogEntry,
    _url: &str,
    _out_path: &Path,
    _known_total: Option<u64>,
    _done_base: u64,
    _total_all: u64,
) -> Result<(), String> {
    Err("curl fallback unavailable".into())
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
    let final_dest = model_dir_for_id(&entry.id, &entry.layout);
    let staging = models_dir().join(format!(".staging-{}", entry.layout));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    emit_progress(
        app,
        &entry,
        2,
        "连接下载源…",
        false,
    );

    let download_result = async {
        if entry.download.is_modelscope() {
            download_from_modelscope(app, &entry, &staging).await
        } else if entry.download.is_archive() {
            download_from_archive(app, &entry, &staging).await
        } else {
            download_from_zip(app, &entry, &staging).await
        }
    }
    .await;

    if let Err(e) = download_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    if !is_model_installed_at(&staging, &entry) {
        let _ = fs::remove_dir_all(&staging);
        return Err("模型文件不完整，请重试下载".into());
    }

    if final_dest.exists() {
        fs::remove_dir_all(&final_dest).map_err(|e| e.to_string())?;
    }
    if let Err(rename_err) = fs::rename(&staging, &final_dest) {
        copy_dir_recursive(&staging, &final_dest)?;
        let _ = fs::remove_dir_all(&staging);
        tracing::warn!("model staging rename failed: {rename_err}");
    }

    if !is_model_installed(&entry) {
        return Err("模型安装校验失败，请重试下载".into());
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
    let denom = file_total.filter(|t| *t > 0).unwrap_or(total_all).max(1);
    let current = done_base.saturating_add(file_done.min(denom));
    (5 + ((current * 90) / denom) as u8).min(95)
}

/// Slow pulse while waiting for the first response bytes (curl / TCP connect).
fn connecting_percent(elapsed_secs: u64) -> u8 {
    let secs = elapsed_secs.min(120);
    (3 + ((secs * 9) / 120) as u8).min(12)
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

    let client = http_client()?;

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
    } else {
        emit_progress(
            app,
            entry,
            file_percent(done_base, total_all, 0, known_total),
            "正在连接…",
            false,
        );
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("连接失败 ({CONNECT_TIMEOUT_SECS}s): {e}"))?;

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
    loop {
        let next_chunk = tokio::time::timeout(
            Duration::from_secs(READ_STALL_TIMEOUT_SECS),
            reader.next(),
        )
        .await
        .map_err(|_| format!("下载停滞超过 {READ_STALL_TIMEOUT_SECS}s"))?;
        let Some(chunk) = next_chunk else {
            break;
        };
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        let meta = PartMeta {
            url: url.to_string(),
            total,
            downloaded,
        };
        let _ = fs::write(&meta_path, serde_json::to_string(&meta).unwrap_or_default());

        let label = out_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("下载中");
        let mb_done = downloaded / (1024 * 1024);
        let message = if let Some(expected) = known_total.or(total) {
            format!("{label}… {mb_done} / {} MB", expected / (1024 * 1024))
        } else {
            format!("{label}… {mb_done} MB")
        };
        emit_progress(
            app,
            entry,
            file_percent(done_base, total_all, downloaded, known_total.or(total)),
            &message,
            false,
        );
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
    let urls = entry.download.candidate_zip_urls();
    let total = entry.download.size_bytes.unwrap_or(1);
    download_with_mirrors(
        app,
        entry,
        urls,
        &archive_path,
        entry.download.size_bytes,
        0,
        total,
    )
    .await?;

    if let Some(expected) = &entry.download.sha256 {
        verify_sha256_file(&archive_path, expected)?;
    }
    emit_progress(app, entry, 96, "正在解压模型…", false);
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
    let urls = entry.download.candidate_zip_urls();
    let total = entry.download.size_bytes.unwrap_or(1);
    download_with_mirrors(
        app,
        entry,
        urls,
        &zip_path,
        entry.download.size_bytes,
        0,
        total,
    )
    .await?;

    if let Some(expected) = &entry.download.sha256 {
        verify_sha256_file(&zip_path, expected)?;
    }
    emit_progress(app, entry, 96, "正在解压模型…", false);
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

    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
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
    let temp = std::env::temp_dir().join(format!("voxtype-extract-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp).map_err(|e| e.to_string())?;

    {
        let file = File::open(zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            let out_path: PathBuf = temp.join(name.trim_start_matches('/'));
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
    }

    let mut children: Vec<PathBuf> = fs::read_dir(&temp)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|d| d.path()))
        .collect();
    let source = if children.len() == 1 && children[0].is_dir() {
        children.remove(0)
    } else {
        temp.clone()
    };

    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    copy_dir_recursive(&source, dest)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(())
}
