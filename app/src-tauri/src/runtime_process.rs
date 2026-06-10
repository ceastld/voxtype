use crate::settings::{
    load_settings, model_dir_for_id, resolve_active_model, resolve_runtime_provider,
    runtime_exe_path, runtime_log_path, save_settings,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const RUNTIME_PORT_MIN: u16 = 6016;
const RUNTIME_PORT_MAX: u16 = 6100;

pub struct RuntimeProcess {
    child: Mutex<Option<Child>>,
    active_port: Mutex<Option<u16>>,
}

impl RuntimeProcess {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            active_port: Mutex::new(None),
        }
    }

    pub fn active_port(&self) -> u16 {
        self.active_port
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or_else(|| load_settings().runtime_ws_port)
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .ok()
            .and_then(|mut g| {
                g.as_mut().map(|c| matches!(c.try_wait(), Ok(None)))
            })
            .unwrap_or(false)
    }

    pub fn is_alive(&self) -> bool {
        self.is_running()
    }

    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(mut port) = self.active_port.lock() {
            *port = None;
        }
    }

    /// Start runtime on an available port. Reclaims only stale `voxtype-runtime.exe` listeners.
    pub fn ensure_started(&self) -> Result<u16, String> {
        if self.is_running() {
            return Ok(self.active_port());
        }
        self.stop();
        let port = resolve_runtime_port(false)?;
        self.start_inner(port)?;
        Ok(port)
    }

    /// Force a new free port (used when health/ws does not match our managed process).
    pub fn restart_on_fresh_port(&self) -> Result<u16, String> {
        self.stop();
        let current = load_settings().runtime_ws_port;
        let port = resolve_runtime_port(true)?;
        if port == current {
            tracing::warn!("no alternate runtime port found; retrying {current}");
        }
        self.start_inner(port)?;
        Ok(port)
    }

    pub fn start(&self) -> Result<(), String> {
        self.ensure_started().map(|_| ())
    }

    fn start_inner(&self, port: u16) -> Result<(), String> {
        let settings = load_settings();
        let exe = runtime_exe_path();
        if !exe.is_file() {
            return Err(format!(
                "未找到识别服务: {} — 请先构建 runtime 或安装完整安装包",
                exe.display()
            ));
        }
        let runtime_cwd = exe
            .parent()
            .ok_or_else(|| format!("识别服务路径无效: {}", exe.display()))?;

        let (model_dir, model_type, _entry) = resolve_active_model_dir(&settings)?;
        let provider = resolve_runtime_provider(settings.use_gpu);
        tracing::info!(
            "starting runtime: exe={} port={} model={} type={} provider={}",
            exe.display(),
            port,
            model_dir.display(),
            model_type,
            provider
        );

        let log_path = runtime_log_path();
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("无法写入 runtime 日志 {}: {e}", log_path.display()))?;
        let _ = writeln!(
            log_file.try_clone().map_err(|e| e.to_string())?,
            "\n--- voxtype runtime start {} port={} model={} provider={} ---",
            chrono_lite_now(),
            port,
            model_dir.display(),
            provider
        );

        let mut cmd = Command::new(&exe);
        cmd.current_dir(runtime_cwd);
        // Model files are downloaded/managed by the Tauri app — never auto-delete on bootstrap.
        cmd.env("VOXTYPE_AUTO_DOWNLOAD_MODEL", "0");
        cmd.arg("--port")
            .arg(port.to_string())
            .arg("--model-dir")
            .arg(&model_dir)
            .arg("--model-type")
            .arg(&model_type)
            .arg("--provider")
            .arg(provider);
        cmd.stdout(Stdio::null()).stderr(Stdio::from(log_file));
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动识别服务失败: {e}"))?;
        *self.child.lock().map_err(|_| "lock poisoned")? = Some(child);
        *self.active_port.lock().map_err(|_| "lock poisoned")? = Some(port);

        if settings.runtime_ws_port != port {
            let mut next = settings;
            next.runtime_ws_port = port;
            save_settings(&next)?;
        }

        Ok(())
    }

    pub async fn wait_until_healthy(port: u16, max_ms: u64) -> Result<(), String> {
        let steps = max_ms / 300;
        for i in 0..steps {
            if let Some(health) = crate::voice_ws::fetch_runtime_health(port).await {
                if health.ready {
                    tracing::info!("runtime ready on port {port}");
                    return Ok(());
                }
                if !health.ok {
                    return Err(format!(
                        "识别服务异常: {}",
                        health.detail.unwrap_or_else(|| "health not ok".into())
                    ));
                }
                if i > 0 && i % 10 == 0 {
                    tracing::info!(
                        "waiting for runtime model load… ({:.0}s)",
                        (i as f64) * 0.3
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        Err("识别服务启动超时（模型加载可能较慢，请查看 runtime 日志）".into())
    }
}

fn resolve_runtime_port(force_new: bool) -> Result<u16, String> {
    let settings = load_settings();
    let configured = settings.runtime_ws_port;

    if force_new {
        let start = configured.saturating_add(1).max(RUNTIME_PORT_MIN);
        let port = find_free_runtime_port(start)?;
        persist_runtime_port(port)?;
        return Ok(port);
    }

    let listeners = find_listener_pids(configured);
    if listeners.is_empty() {
        return Ok(configured);
    }

    if listeners.iter().all(|pid| is_voxtype_runtime_pid(*pid)) {
        tracing::info!("reclaiming stale voxtype-runtime on port {configured}");
        kill_voxtype_runtime_pids(&listeners)?;
        std::thread::sleep(Duration::from_millis(300));
        return Ok(configured);
    }

    let port = find_free_runtime_port(configured.saturating_add(1).max(RUNTIME_PORT_MIN))?;
    tracing::warn!(
        "port {configured} is used by another app; switching VoxType runtime to {port}"
    );
    persist_runtime_port(port)?;
    Ok(port)
}

fn persist_runtime_port(port: u16) -> Result<(), String> {
    let mut settings = load_settings();
    if settings.runtime_ws_port == port {
        return Ok(());
    }
    settings.runtime_ws_port = port;
    save_settings(&settings)
}

fn find_free_runtime_port(start: u16) -> Result<u16, String> {
    let start = start.clamp(RUNTIME_PORT_MIN, RUNTIME_PORT_MAX);
    for port in start..=RUNTIME_PORT_MAX {
        if find_listener_pids(port).is_empty() {
            return Ok(port);
        }
    }
    for port in RUNTIME_PORT_MIN..start {
        if find_listener_pids(port).is_empty() {
            return Ok(port);
        }
    }
    Err(format!(
        "无可用识别服务端口（{RUNTIME_PORT_MIN}-{RUNTIME_PORT_MAX} 均被占用）"
    ))
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn resolve_active_model_dir(
    settings: &crate::settings::AppSettings,
) -> Result<(std::path::PathBuf, String, crate::settings::ModelCatalogEntry), String> {
    let entry = resolve_active_model(settings)?;
    let dir = model_dir_for_id(&entry.id, &entry.layout);
    Ok((
        dir,
        entry.runtime_preset_or_type().to_string(),
        entry,
    ))
}

fn find_listener_pids(port: u16) -> Vec<u32> {
    let Some(output) = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
        .ok()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let needle = format!("127.0.0.1:{port}");
    let mut pids = Vec::new();
    for line in text.lines() {
        if line.contains(&needle) && line.contains("LISTENING") {
            if let Some(pid) = line.split_whitespace().last() {
                if let Ok(pid) = pid.parse::<u32>() {
                    if pid > 0 {
                        pids.push(pid);
                    }
                }
            }
        }
    }
    pids
}

fn process_image_name(pid: u32) -> Option<String> {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if line.is_empty() || line.contains("No tasks") {
        return None;
    }
    let image = line.split(',').next()?;
    Some(image.trim_matches('"').to_string())
}

fn is_voxtype_runtime_pid(pid: u32) -> bool {
    process_image_name(pid)
        .map(|name| name.eq_ignore_ascii_case("voxtype-runtime.exe"))
        .unwrap_or(false)
}

fn kill_voxtype_runtime_pids(pids: &[u32]) -> Result<(), String> {
    for pid in pids {
        if !is_voxtype_runtime_pid(*pid) {
            continue;
        }
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status()
            .map_err(|e| format!("taskkill failed: {e}"))?;
        tracing::warn!(
            "stopped stale voxtype-runtime pid {pid} (exit={})",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}
