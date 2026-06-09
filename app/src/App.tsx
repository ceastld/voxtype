import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type AppStatus = {
  runtimeRunning: boolean;
  runtimeReady: boolean;
  dictationPhase: string;
  activeModelId: string | null;
  activeModelName?: string | null;
  hotkey: string;
  modelsCatalogPath?: string;
  modelsCatalogSource?: string;
};

type ModelStatus = {
  id: string;
  name: string;
  description: string;
  supported: boolean;
  installed: boolean;
  active: boolean;
  capsWriterType?: string | null;
};

export default function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [hotkey, setHotkey] = useState("F9");
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [downloadMessage, setDownloadMessage] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  const refreshModels = useCallback(async () => {
    const list = await invoke<ModelStatus[]>("list_models_status");
    setModels(list);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<AppStatus>("get_app_status");
      setStatus(s);
      setHotkey(s.hotkey);
      await refreshModels();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
    }
  }, [refreshModels]);

  useEffect(() => {
    void refresh();
    const unlisten = listen<{ percent: number; message?: string }>(
      "model-download-progress",
      (ev) => {
        setDownloadProgress(ev.payload.percent);
        if (ev.payload.message) setDownloadMessage(ev.payload.message);
      },
    );
    const unlistenDone = listen("model-download-done", () => {
      setDownloadProgress(null);
      setDownloadMessage(null);
      setBusyId(null);
      void refresh();
    });
    return () => {
      void unlisten.then((fn) => fn());
      void unlistenDone.then((fn) => fn());
    };
  }, [refresh]);

  const applyHotkey = async () => {
    setMessage(null);
    await invoke("set_hotkey", { hotkey });
    await refresh();
    setMessage("热键已更新，重启客户端后全局生效。");
  };

  const downloadModel = async (modelId: string) => {
    setMessage(null);
    setBusyId(modelId);
    setDownloadProgress(0);
    setDownloadMessage("准备下载…");
    try {
      await invoke("download_model", { modelId });
      setMessage("模型下载完成，已自动切换。");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      setDownloadProgress(null);
      setDownloadMessage(null);
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

  return (
    <div className="app">
      <h1>VoxType</h1>
      <p className="sub">离线语音听写 — CapsWriter 同款引擎 · ModelScope 下载</p>

      <div className="card">
        <h2>状态</h2>
        <div className="status">
          识别服务：{" "}
          <span className={status?.runtimeReady ? "ok" : "err"}>
            {status?.runtimeReady
              ? "就绪"
              : status?.runtimeRunning
                ? "启动中"
                : "未运行"}
          </span>
        </div>
        <div className="status">听写：{status?.dictationPhase ?? "—"}</div>
        <div className="status">
          当前模型：{status?.activeModelName ?? status?.activeModelId ?? "未选择"}
        </div>
        {status?.modelsCatalogSource && (
          <div className="status" title={status.modelsCatalogPath}>
            模型目录：{status.modelsCatalogSource === "bundled" ? "安装包内置" : status.modelsCatalogSource}
          </div>
        )}
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" onClick={() => void refresh()}>
            刷新
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => void invoke("restart_runtime").then(() => refresh())}
          >
            重启识别服务
          </button>
        </div>
      </div>

      <div className="card">
        <h2>热键</h2>
        <div className="row">
          <label htmlFor="hotkey">按住说话</label>
          <input
            id="hotkey"
            value={hotkey}
            onChange={(e) => setHotkey(e.target.value)}
            placeholder="F9"
          />
          <button type="button" onClick={() => void applyHotkey()}>
            保存
          </button>
        </div>
      </div>

      <div className="card">
        <h2>识别模型（与 CapsWriter 对齐）</h2>
        <p className="status" style={{ marginBottom: 12 }}>
          下载地址来自安装包内置 catalog；权重从 ModelScope 拉取。Fun-ASR / Qwen3 引擎开发中。
        </p>
        {models.map((m) => (
          <div
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
                {m.capsWriterType && (
                  <span className="badge type">{m.capsWriterType}</span>
                )}
              </span>
            </div>
            <div className="status">{m.description}</div>
            <div className="row" style={{ marginTop: 8 }}>
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
                切换使用
              </button>
            </div>
          </div>
        ))}
        {downloadProgress !== null && (
          <>
            <div className="progress">
              <span style={{ width: `${downloadProgress}%` }} />
            </div>
            {downloadMessage && (
              <p className="status">{downloadMessage}</p>
            )}
          </>
        )}
      </div>

      {message && <p className="status">{message}</p>}
    </div>
  );
}
