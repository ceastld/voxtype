import { useCallback, useEffect, useRef, useState } from "react";
import {
  formatHotkeyDisplay,
  keyboardEventToHotkey,
} from "../lib/hotkey-format";

type HotkeyRecorderProps = {
  value: string;
  disabled?: boolean;
  onCapture: (hotkey: string) => void | Promise<void>;
};

export function HotkeyRecorder({
  value,
  disabled = false,
  onCapture,
}: HotkeyRecorderProps) {
  const [recording, setRecording] = useState(false);
  const [busy, setBusy] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!recording) return;
    buttonRef.current?.focus();
  }, [recording]);

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLButtonElement>) => {
      if (!recording || disabled || busy) return;
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecording(false);
        return;
      }

      const captured = keyboardEventToHotkey(event.nativeEvent);
      if (!captured || captured === value) {
        if (captured === value) setRecording(false);
        return;
      }

      setRecording(false);
      setBusy(true);
      void Promise.resolve(onCapture(captured)).finally(() => setBusy(false));
    },
    [recording, disabled, busy, value, onCapture],
  );

  const label = recording
    ? "按下新快捷键…"
    : busy
      ? "保存中…"
      : formatHotkeyDisplay(value);

  return (
    <button
      ref={buttonRef}
      type="button"
      className={`hotkey-recorder${recording ? " recording" : ""}`}
      disabled={disabled || busy}
      aria-label={recording ? "正在录制快捷键" : `当前快捷键 ${label}`}
      onClick={() => setRecording(true)}
      onKeyDown={handleKeyDown}
      onBlur={() => setRecording(false)}
    >
      <span className="hotkey-recorder-label">{label}</span>
      {recording && (
        <span className="hotkey-recorder-hint">Esc 取消</span>
      )}
    </button>
  );
}
