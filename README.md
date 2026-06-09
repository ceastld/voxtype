# VoxType

Offline voice dictation for Windows — hold a hotkey, speak, release to **type into the focused window**.

Inspired by [CapsWriter-Offline](https://github.com/HaujetZhao/CapsWriter-Offline). ASR runtime uses [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) (SenseVoice / Paraformer).

## Components

| Path | Role |
|------|------|
| `runtime/` | Python ASR server (`voxtype-runtime.exe`, WebSocket `voxtype-voice-v1`) |
| `app/` | Tauri client — settings, overlay, hotkeys, direct typing, Quicker HTTP API |
| `plugin/` | Quicker plugin — button triggers dictation |
| `catalog/models.json` | Public model catalog for settings UI |

## Quick start (dev)

```powershell
# 1. ASR runtime
cd runtime
uv sync
uv run download-asr-model
uv run voxtype-runtime

# 2. Tauri client (new terminal)
cd app
pnpm install
pnpm tauri dev
```

- Runtime health: `http://127.0.0.1:6016/health`
- Client API: `http://127.0.0.1:6020/health`
- Default hotkey: **F9** (hold to dictate, release to type)

Data directory: `%LOCALAPPDATA%\VoxType\` (models, settings).

## Model download

Model **weights** are not bundled (too large). Model **download URLs** are written in `catalog/models.json` and ship inside the installer at `catalog/models.json` — the app reads this file locally (no remote catalog fetch). Users download weights from the settings UI on first use.

## Release

Push tag `v0.1.x` → GitHub Actions builds a **single NSIS installer** (`VoxType_<version>_x64-setup.exe`) that includes:

- Tauri desktop app (settings, overlay, hotkeys, typing)
- `voxtype-runtime` (PyInstaller onedir, next to the app under `runtime/voxtype-runtime/`)
- `catalog/models.json`

```powershell
git tag v0.1.3
git push origin v0.1.3
```

Local full build:

```powershell
cd runtime && pwsh -NoProfile -File ./scripts/build-win.ps1
cd ../app && pnpm install && pnpm tauri build
```

## Quicker integration

Install the `plugin/` package, bind a button to `VoxType.Plugin.Launcher.Start`.  
Or use the bundled Quicker action (see `scripts/setup-quicker-action.ps1`).

## License

MIT
