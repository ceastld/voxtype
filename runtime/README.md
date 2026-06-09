# voxtype-runtime

Local ASR server for [VoxType](../README.md) (`voxtype-voice-v1` WebSocket protocol).

## Dev

```powershell
uv sync
uv run download-asr-model
uv run voxtype-runtime
```

Health: `http://127.0.0.1:6016/health`

## Build Windows exe

```powershell
pwsh -NoProfile -File ./scripts/build-win.ps1
```

Output: `dist/voxtype-runtime/voxtype-runtime.exe`
