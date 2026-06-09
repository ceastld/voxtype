type DownloadToastProps = {
  modelName: string;
  percent: number;
  message: string;
  visible: boolean;
};

export function DownloadToast({
  modelName,
  percent,
  message,
  visible,
}: DownloadToastProps) {
  if (!visible) return null;

  return (
    <div className="download-toast" role="status" aria-live="polite">
      <div className="download-toast-head">
        <span className="download-toast-title">正在下载</span>
        <span className="download-toast-name">{modelName}</span>
        <span className="download-toast-pct">{percent}%</span>
      </div>
      <div className="download-toast-bar">
        <span style={{ width: `${Math.min(100, Math.max(0, percent))}%` }} />
      </div>
      <p className="download-toast-msg">{message}</p>
    </div>
  );
}
