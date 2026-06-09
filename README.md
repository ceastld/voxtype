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

## Release

Push tag `v0.1.0` → GitHub Actions builds runtime zip + NSIS installer.

```powershell
git tag v0.1.0
git push origin v0.1.0
```

## Quicker integration

Install the `plugin/` package, bind a button to `VoxType.Plugin.Launcher.Start`.  
Or use the bundled Quicker action (see `scripts/setup-quicker-action.ps1`).

## License

MIT
