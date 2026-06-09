import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

const WINDOW_WIDTH = 760;
const MIN_HEIGHT = 420;
const MAX_HEIGHT = 880;
const HEIGHT_PAD = 12;

async function fitSettingsWindow(root: HTMLElement | null) {
  if (!root) return;
  try {
    const win = getCurrentWindow();
    if (win.label !== "main") return;

    const contentHeight = Math.ceil(root.getBoundingClientRect().height);
    const target = Math.min(
      MAX_HEIGHT,
      Math.max(MIN_HEIGHT, contentHeight + HEIGHT_PAD),
    );

    const size = await win.innerSize();
    if (Math.abs(size.height - target) < 4 && size.width === WINDOW_WIDTH) {
      return;
    }

    await win.setSize(new LogicalSize(WINDOW_WIDTH, target));
  } catch {
    // Not running inside Tauri (e.g. browser preview)
  }
}

export function useSettingsWindowSize(deps: unknown[] = []) {
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const schedule = () => {
      requestAnimationFrame(() => {
        void fitSettingsWindow(root);
      });
    };

    schedule();

    const observer = new ResizeObserver(schedule);
    observer.observe(root);

    const onResize = () => schedule();
    window.addEventListener("resize", onResize);

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return rootRef;
}
