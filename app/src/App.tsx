import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DownloadToast } from "./components/DownloadToast";
import { useSettingsWindowSize } from "./lib/useSettingsWindowSize";

type RuntimeHealth = {
  executionProvider?: string | null;
};

type AppStatus = {
  runtimeRunning: boolean;
  runtimeReady: boolean;
  runtimeWsPort?: number;
  runtimeHealth?: RuntimeHealth | null;
  dictationPhase: string;
  activeModelId: string | null;
  activeModelName?: string | null;
  hotkey: string;
  hotkeyMode?: string;
  useGpu?: boolean;
  requestedProvider?: string;
  lastError?: string | null;
  modelsCatalogSource?: string;
};

type ModelStatus = {
  id: string;
  name: string;
  description: string;
  supported: boolean;
  installed: boolean;
  active: boolean;
};

type DownloadProgress = {
  percent: number;
  message?: string;
  modelId?: string;
  modelName?: string;
  done?: boolean;
};

export default function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [hotkey, setHotkey] = useState("F9");
  const [hotkeyMode, setHotkeyMode] = useState<"hold" | "toggle">("hold");
  const [useGpu, setUseGpu] = useState(true);
  const [download, setDownload] = useState<DownloadProgress | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const rootRef = useSettingsWindowSize([
    models.length,
    status?.lastError,
    message,
    download,
  ]);

  const refreshModels = useCallback(async () => {
    const list = await invoke<ModelStatus[]>("list_models_status");
    setModels(list);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<AppStatus>("get_app_status");
      setStatus(s);
      setHotkey(s.hotkey);
      setHotkeyMode(s.hotkeyMode === "toggle" ? "toggle" : "hold");
      setUseGpu(s.useGpu !== false);
      await refreshModels();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
    }
  }, [refreshModels]);

  useEffect(() => {
    void refresh();
    const unlisten = listen<DownloadProgress>("model-download-progress", (ev) => {
      setDownload((prev) => ({
        percent: ev.payload.percent,
        message: ev.payload.message ?? prev?.message ?? "下载中…",
        modelId: ev.payload.modelId ?? prev?.modelId,
        modelName: ev.payload.modelName ?? prev?.modelName ?? "模型",
        done: ev.payload.done ?? false,
      }));
    });
    const unlistenDone = listen("model-download-done", () => {
      setDownload(null);
      setBusyId(null);
      void refresh();
      setMessage("模型下载完成，已自动切换。");
    });
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
    };
  }, [refresh]);

  const applyUseGpu = async (enabled: boolean) => {
    setMessage(null);
    setUseGpu(enabled);
    try {
      await invoke("set_use_gpu", { useGpu: enabled });
      await refresh();
      setMessage(
        enabled
          ? "已启用 GPU 加速并重启识别服务。"
          : "已切换为 CPU 推理并重启识别服务。",
      );
    } catch (e) {
      setUseGpu(!enabled);
      setMessage(e instanceof Error ? e.message : String(e));
    }
  };

  const applyHotkey = async () => {
    setMessage(null);
    await invoke("set_hotkey", { hotkey });
    await invoke("set_hotkey_mode", { mode: hotkeyMode });
    await refresh();
    setMessage("热键已更新，请完全退出后重新打开 VoxType 以生效。");
  };

  const downloadModel = async (modelId: string) => {
    const model = models.find((m) => m.id === modelId);
    setMessage(null);
    setBusyId(modelId);
    setDownload({
      percent: 0,
      message: "准备下载…",
      modelId,
      modelName: model?.name ?? modelId,
    });
    try {
      await invoke("download_model", { modelId });
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      setDownload(null);
      setBusyId(null);
    }
  };

  const switchModel = async (modelId: string) => {
    setMessage(null);
    setBusyId(modelId);
    try {
      await invoke("activate_model", { modelId });
      await invoke("restart_runtime");
      await refresh();
      setMessage("已切换模型并重启识别服务。");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  const runtimeLabel = status?.runtimeReady
    ? "就绪"
    : status?.runtimeRunning
      ? "启动中"
      : "未运行";

  const executionProvider =
    status?.runtimeHealth?.executionProvider ??
    (status?.runtimeReady ? "cpu" : null);

  const providerLabel =
    executionProvider === "cuda"
      ? "CUDA"
      : executionProvider === "coreml"
        ? "CoreML"
        : executionProvider === "directml"
          ? "DirectML"
          : executionProvider === "cpu"
            ? "CPU"
            : executionProvider;

  return (
    <div className="app" ref={rootRef}>
      <header className="app-header">
        <div className="brand">
          <div className="brand-icon" aria-hidden />
          <div>
            <h1>VoxType</h1>
            <p className="sub">离线语音听写 · 按住热键说话</p>
          </div>
        </div>
        <div className="header-actions">
          <button type="button" className="ghost" onClick={() => void refresh()}>
            刷新
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => void invoke("restart_runtime").then(() => refresh())}
          >
            重启服务
          </button>
        </div>
      </header>

      <div className="layout">
        <section className="card compact">
          <h2>运行状态</h2>
          <div className="stat-grid">
            <div className="stat">
              <span className="label">识别服务</span>
              <span className={status?.runtimeReady ? "value ok" : "value warn"}>
                {runtimeLabel}
                {status?.runtimeWsPort ? ` · ${status.runtimeWsPort}` : ""}
              </span>
            </div>
            <div className="stat">
              <span className="label">听写状态</span>
              <span className="value">{status?.dictationPhase ?? "—"}</span>
            </div>
            <div className="stat">
              <span className="label">当前模型</span>
              <span className="value">
                {status?.activeModelName ?? status?.activeModelId ?? "未选择"}
              </span>
            </div>
            <div className="stat">
              <span className="label">推理后端</span>
              <span className="value">
                {providerLabel ?? "—"}
                {status?.useGpu === false ? " · 已关闭 GPU" : ""}
              </span>
            </div>
            <div className="stat">
              <span className="label">目录来源</span>
              <span className="value">
                {status?.modelsCatalogSource === "bundled"
                  ? "安装包内置"
                  : (status?.modelsCatalogSource ?? "—")}
              </span>
            </div>
          </div>
          {status?.lastError && (
            <p className="inline-alert err">{status.lastError}</p>
          )}
        </section>

        <section className="card compact">
          <h2>推理</h2>
          <div className="toggle-row">
            <label className="toggle" htmlFor="useGpu">
              <input
                id="useGpu"
                type="checkbox"
                checked={useGpu}
                onChange={(e) => void applyUseGpu(e.target.checked)}
              />
              <span className="toggle-track" aria-hidden />
              <span className="toggle-label">GPU 加速</span>
            </label>
            <span className="hint inline">
              默认开启；无可用 GPU 时自动回退 CPU
              {status?.requestedProvider
                ? `（请求 ${status.requestedProvider}）`
                : ""}
            </span>
          </div>
        </section>

        <section className="card compact">
          <h2>热键</h2>
          <div className="form-grid">
            <label htmlFor="hotkey">快捷键</label>
            <input
              id="hotkey"
              value={hotkey}
              onChange={(e) => setHotkey(e.target.value)}
              placeholder="F9"
            />
            <label htmlFor="hotkeyMode">触发方式</label>
            <div className="inline-field">
              <select
                id="hotkeyMode"
                value={hotkeyMode}
                onChange={(e) =>
                  setHotkeyMode(e.target.value === "toggle" ? "toggle" : "hold")
                }
              >
                <option value="hold">按住说话，松开结束</option>
                <option value="toggle">按一下开始，再按一下结束</option>
              </select>
              <button type="button" onClick={() => void applyHotkey()}>
                保存
              </button>
            </div>
          </div>
          <p className="hint">悬浮窗在说话时显示实时识别文字，不抢焦点。</p>
        </section>

        <section className="card models-card">
          <div className="section-head">
            <h2>识别模型</h2>
            <span className="hint">ModelScope 下载 · 支持断点续传</span>
          </div>
          <div className="model-grid">
            {models.map((m) => (
              <article
                key={m.id}
                className={`model-item${m.active ? " active" : ""}${!m.supported ? " disabled" : ""}`}
              >
                <div className="model-head">
                  <strong>{m.name}</strong>
                  <span className="badges">
                    {m.active && <span className="badge on">使用中</span>}
                    {m.installed && !m.active && (
                      <span className="badge ok">已安装</span>
                    )}
                    {!m.supported && <span className="badge soon">即将支持</span>}
                  </span>
                </div>
                <p className="model-desc">{m.description}</p>
                <div className="model-actions">
                  <button
                    type="button"
                    disabled={!m.supported || busyId !== null}
                    onClick={() => void downloadModel(m.id)}
                  >
                    {m.installed ? "重新下载" : "下载"}
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    disabled={
                      !m.supported || !m.installed || m.active || busyId !== null
                    }
                    onClick={() => void switchModel(m.id)}
                  >
                    切换
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>
      </div>

      {message && <p className="footer-msg">{message}</p>}

      <DownloadToast
        visible={download !== null}
        modelName={download?.modelName ?? "模型"}
        percent={download?.percent ?? 0}
        message={download?.message ?? ""}
      />
    </div>
  );
}
