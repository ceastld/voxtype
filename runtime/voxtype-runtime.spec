# PyInstaller spec â€?voxtype-runtime (Windows x64, onedir)
# Run: uv run pyinstaller voxtype-runtime.spec

from __future__ import annotations

from pathlib import Path

from PyInstaller.utils.hooks import collect_all

ROOT = Path(SPECPATH)
ENTRY = ROOT / "packaging" / "runtime_entry.py"
MODEL_IDENTITY = ROOT / "models" / "sensevoice-model-identity.json"

sherpa_datas, sherpa_binaries, sherpa_hiddenimports = collect_all("sherpa_onnx")
bundle_datas = [*sherpa_datas, (str(MODEL_IDENTITY), "models")]

a = Analysis(
    [str(ENTRY)],
    pathex=[str(ROOT / "src")],
    binaries=sherpa_binaries,
    datas=bundle_datas,
    hiddenimports=[
        *sherpa_hiddenimports,
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
        "voxtype_runtime",
        "voxtype_runtime.__main__",
        "voxtype_runtime.server",
        "voxtype_runtime.session",
        "voxtype_runtime.protocol",
        "voxtype_runtime.config",
        "voxtype_runtime.paths",
        "voxtype_runtime.download_model",
        "voxtype_runtime.recognizer",
        "voxtype_runtime.recognizer.stub",
        "voxtype_runtime.recognizer.sherpa_onnx",
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
