import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type AppStatus = {
  runtimeRunning: boolean;
  runtimeReady: boolean;
  dictationPhase: string;
  activeModelId: string | null;
  hotkey: string;
};

type ModelEntry = {
  id: string;
  name: string;
  description: string;
  default?: boolean;
};

type ModelsCatalog = {
  models: ModelEntry[];
};

export default function App() {
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [catalog, setCatalog] = useState<ModelEntry[]>([]);
  const [hotkey, setHotkey] = useState("F9");
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<AppStatus>("get_app_status");
      setStatus(s);
      setHotkey(s.hotkey);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    void invoke<ModelsCatalog>("load_models_catalog").then((c) =>
      setCatalog(c.models),
    );
    const unlisten = listen<{ percent: number }>(
      "model-download-progress",
      (ev) => setDownloadProgress(ev.payload.percent),
    );
    const unlistenDone = listen("model-download-done", () => {
      setDownloadProgress(null);
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
    setDownloadProgress(0);
    try {
      await invoke("download_model", { modelId });
      setMessage("模型下载完成。");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : String(e));
      setDownloadProgress(null);
    }
  };

  const activateModel = async (modelId: string) => {
    setMessage(null);
    await invoke("activate_model", { modelId });
    await refresh();
    setMessage("已切换模型，正在重启识别服务…");
  };

  return (
    <div className="app">
      <h1>VoxType</h1>
      <p className="sub">离线语音听写 — 按住热键说话，松手直接上屏</p>

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
          当前模型：{status?.activeModelId ?? "未选择"}
        </div>
        <div className="row" style={{ marginTop: 12 }}>
          <button type="button" onClick={() => void refresh()}>
            刷新
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => void invoke("restart_runtime")}
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
        <p className="status">默认 F9；松开热键后文字输入到当前焦点窗口。</p>
      </div>

      <div className="card">
        <h2>模型</h2>
        {catalog.map((m) => (
          <div
            key={m.id}
            className={`model-item${status?.activeModelId === m.id ? " active" : ""}`}
          >
            <strong>{m.name}</strong>
            <div className="status">{m.description}</div>
            <div className="row" style={{ marginTop: 8 }}>
              <button type="button" onClick={() => void downloadModel(m.id)}>
                下载
              </button>
              <button
                type="button"
                className="secondary"
                onClick={() => void activateModel(m.id)}
              >
                启用
              </button>
            </div>
          </div>
        ))}
        {downloadProgress !== null && (
          <div className="progress">
            <span style={{ width: `${downloadProgress}%` }} />
          </div>
        )}
      </div>

      {message && <p className="status">{message}</p>}
    </div>
  );
}
