# voxtype-runtime

Local ASR server for [VoxType](../README.md) (`voxtype-voice-v1` WebSocket protocol).

## Engine backends

| Backend | Models | Shipped in installer |
|---------|--------|----------------------|
| `sherpa_onnx` (default) | SenseVoice, Paraformer | Yes |
| `onnx_gguf` (planned) | Fun-ASR-Nano, Qwen3-ASR | No — dev-only until a separate add-on pack |

Set `VOXTYPE_ENGINE_BACKEND=onnx_gguf` only when developing hybrid models locally.

## Dev

```powershell
uv sync --extra onnx-gguf
pwsh -NoProfile -File ./scripts/install-llama-cpp.ps1
pwsh -NoProfile -File ./scripts/export-onnx-gguf-models.ps1 -Preset all
uv run voxtype-runtime
```

SenseVoice / Paraformer (sherpa only):

```powershell
uv run download-asr-model --preset sensevoice
uv run voxtype-runtime
```

Health: `http://127.0.0.1:6016/health`

## ONNX+GGUF hybrid export

FunASR-Nano and Qwen3-ASR use a **split architecture**:

- **ONNX Runtime**: audio encoder (+ CTC for FunASR)
- **llama.cpp + GGUF**: LLM decoder (Vulkan/CUDA on Windows)

Export once (needs PyTorch + ModelScope weights):

```powershell
cd runtime
pwsh -NoProfile -File ./scripts/export-onnx-gguf-models.ps1 -Preset fun_asr_nano
# or qwen_asr / all
```

Models land in `%LOCALAPPDATA%\VoxType\models\fun-asr-nano` and `qwen-asr`.

If hybrid files are missing, runtime **falls back** to sherpa-onnx when a sherpa pack is present.

## Build Windows exe

```powershell
pwsh -NoProfile -File ./scripts/build-win.ps1
```

Output: `dist/voxtype-runtime/voxtype-runtime.exe`
