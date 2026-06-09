const MODIFIER_ONLY_KEYS = new Set([
  "Control",
  "Shift",
  "Alt",
  "Meta",
  "OS",
]);

const LONE_KEY_TOKENS = new Set(
  Array.from({ length: 24 }, (_, i) => `F${i + 1}`).concat([
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Pause",
    "PrintScreen",
    "ScrollLock",
  ]),
);

const KEY_ALIASES: Record<string, string> = {
  " ": "Space",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Enter: "Enter",
  Escape: "Escape",
  Backspace: "Backspace",
  Delete: "Delete",
  Tab: "Tab",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Insert: "Insert",
  ",": "Comma",
  ".": "Period",
  ";": "Semicolon",
  "'": "Quote",
  "[": "BracketLeft",
  "]": "BracketRight",
  "\\": "Backslash",
  "/": "Slash",
  "-": "Minus",
  "=": "Equal",
  "`": "Backquote",
};

function normalizeShortcutKey(event: KeyboardEvent): string | null {
  if (event.code.startsWith("Key") && event.code.length === 4) {
    return event.code.slice(3).toUpperCase();
  }
  if (event.code.startsWith("Digit") && event.code.length === 6) {
    return event.code.slice(5);
  }
  if (event.code.startsWith("F") && /^F\d+$/.test(event.code)) {
    return event.code;
  }
  if (KEY_ALIASES[event.key]) return KEY_ALIASES[event.key];
  if (event.key.length === 1 && /[a-z0-9]/i.test(event.key)) {
    return event.key.toUpperCase();
  }
  return null;
}

/** Map a keydown event to a Tauri global-shortcut string, or null if invalid. */
export function keyboardEventToHotkey(event: KeyboardEvent): string | null {
  if (event.isComposing) return null;
  if (MODIFIER_ONLY_KEYS.has(event.key)) return null;

  const keyToken = normalizeShortcutKey(event);
  if (!keyToken) return null;

  const parts: string[] = [];
  if (event.ctrlKey || event.metaKey) parts.push("CommandOrControl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  if (parts.length === 0) {
    return LONE_KEY_TOKENS.has(keyToken) ? keyToken : null;
  }

  parts.push(keyToken);
  return parts.join("+");
}

export function isValidHotkeyFormat(shortcut: string): boolean {
  const trimmed = shortcut.trim();
  if (!trimmed) return false;
  const parts = trimmed.split("+").filter(Boolean);
  if (parts.length === 0) return false;
  const key = parts[parts.length - 1];
  if (!key || MODIFIER_ONLY_KEYS.has(key)) return false;
  if (parts.length === 1) return LONE_KEY_TOKENS.has(key);
  const modifiers = parts.slice(0, -1);
  const allowed = new Set([
    "CommandOrControl",
    "Control",
    "Command",
    "Alt",
    "Shift",
    "Super",
  ]);
  const hasPrimary = modifiers.some((part) =>
    ["CommandOrControl", "Control", "Command", "Alt", "Super"].includes(part),
  );
  const shiftWithFunctionKey =
    modifiers.length === 1 &&
    modifiers[0] === "Shift" &&
    /^F\d+$/.test(key);

  return (
    modifiers.length > 0 &&
    modifiers.every((part) => allowed.has(part)) &&
    (hasPrimary || shiftWithFunctionKey)
  );
}

/** Human-readable label for the settings UI (Windows). */
export function formatHotkeyDisplay(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => {
      if (part === "CommandOrControl" || part === "Control") return "Ctrl";
      if (part === "Command") return "Win";
      if (part === "Shift") return "Shift";
      if (part === "Alt") return "Alt";
      if (part === "Super") return "Win";
      if (part === "Space") return "Space";
      return part;
    })
    .join(" + ");
}
