# Models directory

Place offline ASR model files here. The runtime reads `VOXTYPE_MODEL_DIR` (defaults to auto-detect under this folder).

## Default model (SenseVoice int8)

Pinned model: **`sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17`** (k2-fsa official ONNX export).

Fingerprints are in [`sensevoice-model-identity.json`](sensevoice-model-identity.json). Any mirror (Bitiful zip, ModelScope, GitHub tar.bz2) must match these file hashes:

| File | SHA256 |
|------|--------|
| `model.int8.onnx` | `c71f0ce0â€?cd51` (239,233,841 bytes) |
| `tokens.txt` | `f449eb28â€¦a1dc` (315,894 bytes) |

Domestic mirrors for the **same** files:

- ModelScope: [pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue](https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue)
- k2-fsa archive (via ghfast): `sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2`

Download (~228 MB int8, includes ITN / punctuation):

```powershell
cd voice-asr-runtime
uv run download-asr-model
# or from agent-gui:
pnpm voice:download-model
```

Expected layout:

```text
models/sensevoice/
  model.int8.onnx
  tokens.txt
```

Set `VOXTYPE_ASR_MODEL=paraformer` before download to fetch the smaller Paraformer zh-small model instead.

## Sherpa-ONNX dependency

PyPI's `sherpa-onnx` wheel bundles an older ONNX Runtime that cannot load current models.
This project pins wheels from [k2-fsa CPU index](https://k2-fsa.github.io/sherpa/onnx/cpu.html) via `uv sync`.

## Fallback

If SenseVoice is missing, `models/paraformer-zh/` is used automatically when present.

Without any model, the runtime uses a **stub** recognizer (protocol / UI testing only).
