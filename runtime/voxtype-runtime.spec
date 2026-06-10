# PyInstaller spec ??voxtype-runtime (Windows x64, onedir)
# Run: uv run pyinstaller voxtype-runtime.spec

from __future__ import annotations

from pathlib import Path

from PyInstaller.utils.hooks import collect_all, collect_submodules

ROOT = Path(SPECPATH)
REPO_ROOT = ROOT.parent
ENTRY = ROOT / "packaging" / "runtime_entry.py"
MODEL_IDENTITY = ROOT / "models" / "sensevoice-model-identity.json"
QWEN_THIRD_PARTY = REPO_ROOT / "third_party" / "Qwen3-ASR-GGUF"
FUN_THIRD_PARTY = REPO_ROOT / "third_party" / "Fun-ASR-GGUF"

sherpa_datas, sherpa_binaries, sherpa_hiddenimports = collect_all("sherpa_onnx")
onnx_datas, onnx_binaries, onnx_hiddenimports = collect_all("onnxruntime")
gguf_datas, gguf_binaries, gguf_hiddenimports = collect_all("gguf")
scipy_datas, scipy_binaries, scipy_hiddenimports = collect_all("scipy")
qwen_hiddenimports = collect_submodules("qwen_asr_gguf")
voxtype_hiddenimports = collect_submodules("voxtype_runtime")
bundle_datas = [
    *sherpa_datas,
    *onnx_datas,
    *gguf_datas,
    *scipy_datas,
    (str(MODEL_IDENTITY), "models"),
]
if QWEN_THIRD_PARTY.is_dir():
    bundle_datas.append((str(QWEN_THIRD_PARTY), "third_party/Qwen3-ASR-GGUF"))
if FUN_THIRD_PARTY.is_dir():
    bundle_datas.append((str(FUN_THIRD_PARTY), "third_party/Fun-ASR-GGUF"))

a = Analysis(
    [str(ENTRY)],
    pathex=[str(ROOT / "src")],
    binaries=[*sherpa_binaries, *onnx_binaries, *gguf_binaries, *scipy_binaries],
    datas=bundle_datas,
    hiddenimports=[
        *sherpa_hiddenimports,
        *onnx_hiddenimports,
        *gguf_hiddenimports,
        *scipy_hiddenimports,
        *qwen_hiddenimports,
        "soundfile",
        "sentencepiece",
        "pypinyin",
        "srt",
        "pydub",
        "rich",
        "aiohttp",
        "aiohttp.web",
        "multidict",
        "yarl",
        "frozenlist",
        "aiosignal",
        "async_timeout",
        "charset_normalizer",
        "idna",
        "numpy",
        *voxtype_hiddenimports,
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=["pytest", "pygments"],
    noarchive=False,
    optimize=0,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="voxtype-runtime",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    upx_exclude=[],
    name="voxtype-runtime",
)
