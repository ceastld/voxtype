# Model catalog (`models.json`)

**Model download URLs live in this file and ship inside the installer** at `<install-dir>/catalog/models.json`. The app never fetches a remote catalog; it only reads this bundled JSON (or a user override). **Speech model weights** are downloaded on first use via the embedded ModelScope URLs (`modelscopeResolveBase` + `modelscopeFiles`).

## CapsWriter-aligned engines

| `capsWriterType` | VoxType status | Runtime |
|------------------|----------------|---------|
| `sensevoice` | Supported | sherpa-onnx |
| `paraformer` | Supported | sherpa-onnx |
| `fun_asr_nano` | Planned | ONNX + GGUF |
| `qwen_asr` | Planned | ONNX + GGUF |

## Example entry

```json
{
  "id": "sensevoice-int8",
  "supported": true,
  "download": {
    "source": "modelscope",
    "modelscopeResolveBase": "https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master",
    "modelscopeFiles": [
      { "name": "model.int8.onnx", "required": true },
      { "name": "tokens.txt", "required": true }
    ]
  }
}
```

## Override without reinstall

Copy to `%LOCALAPPDATA%\VoxType\catalog\models.json` and restart VoxType.
