use crate::settings::{load_catalog, load_settings, model_dir_for_id, runtime_exe_path};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct RuntimeProcess {
    child: Mutex<Option<Child>>,
}

impl RuntimeProcess {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .ok()
            .and_then(|mut g| {
                g.as_mut().map(|c| {
                    matches!(c.try_wait(), Ok(None))
                })
            })
            .unwrap_or(false)
    }

    pub fn stop(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    pub fn start(&self) -> Result<(), String> {
        if self.is_running() {
            return Ok(());
        }
        self.stop();

        let settings = load_settings();
        let exe = runtime_exe_path();
        if !exe.exists() {
            return Err(format!(
                "未找到识别服务: {exe:?} — 请先构建 runtime 或安装完整包"
            ));
        }

        let model_dir = resolve_active_model_dir(&settings)?;
        let mut cmd = Command::new(&exe);
        cmd.arg("--port")
            .arg(settings.runtime_ws_port.to_string())
            .arg("--model-dir")
            .arg(&model_dir);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd.spawn().map_err(|e| format!("启动识别服务失败: {e}"))?;
        *self.child.lock().map_err(|_| "lock poisoned")? = Some(child);

        Ok(())
    }

    pub async fn wait_until_healthy(port: u16, max_ms: u64) -> bool {
        let steps = max_ms / 250;
        for _ in 0..steps {
            if crate::voice_ws::check_runtime_health(port).await {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        false
    }
}

fn resolve_active_model_dir(
    settings: &crate::settings::AppSettings,
) -> Result<std::path::PathBuf, String> {
    let catalog = load_catalog()?;
    let active_id = settings
        .active_model_id
        .as_deref()
        .or_else(|| catalog.models.iter().find(|m| m.default).map(|m| m.id.as_str()))
        .ok_or_else(|| "未配置模型".to_string())?;
    let entry = catalog
        .models
        .iter()
        .find(|m| m.id == active_id)
        .ok_or_else(|| format!("未知模型: {active_id}"))?;
    let dir = model_dir_for_id(&entry.id, &entry.layout);
    if !dir.exists() {
        return Err(format!("模型目录不存在: {dir:?} — 请先在设置中下载模型"));
    }
    Ok(dir)
}
