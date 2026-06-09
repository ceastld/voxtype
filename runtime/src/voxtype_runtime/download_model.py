"""Download offline ASR model for sherpa-onnx."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path
from typing import Any

from voxtype_runtime.paths import plugin_data_root, sensevoice_identity_path

SENSEVOICE_ARCHIVE_URL = (
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/"
    "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"
)
SENSEVOICE_MODELSCOPE_RESOLVE = (
    "https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/resolve/master"
)
PARAFORMER_MODELSCOPE_RESOLVE = (
    "https://www.modelscope.cn/models/pengzhendong/sherpa-onnx-paraformer-zh-small/resolve/master"
)
PARAFORMER_MODELSCOPE_FILES = (
    "model.int8.onnx",
    "model.onnx",
    "tokens.txt",
    "am.mvn",
    "config.yaml",
)

MODEL_PRESETS: dict[str, dict[str, str]] = {
    "sensevoice": {
        "url": SENSEVOICE_ARCHIVE_URL,
        "dir": "sensevoice",
        "label": "SenseVoice int8 (~228MB, zh/en/ja/ko/yue + ITN/punctuation)",
    },
    "paraformer": {
        "url": (
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/"
            "sherpa-onnx-paraformer-zh-small-2024-03-09.tar.bz2"
        ),
        "dir": "paraformer-zh",
        "label": "Paraformer zh-small (~76MB)",
    },
}
DEFAULT_PRESET = "sensevoice"
REQUIRED_FILES = ("tokens.txt",)
MODEL_FILE_CANDIDATES = ("model.int8.onnx", "model.onnx")
OPTIONAL_FILES = ("am.mvn", "config.yaml")
# paraformer-zh-small ONNX is tens of MB; partial downloads are often 1–10 MB.
PARAFORMER_MIN_ONNX_BYTES = 20 * 1024 * 1024
PARAFORMER_MIN_TOKENS_BYTES = 64
PROGRESS_MARKER = "VOXTYPE_PROGRESS"


def _configure_stdio_utf8() -> None:
    """Windows CI defaults to cp1252; progress messages use CJK text."""
    if sys.platform != "win32":
        return
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is None:
            continue
        try:
            reconfigure(encoding="utf-8", errors="replace")
        except (OSError, ValueError):
            pass


_configure_stdio_utf8()


def report_download_progress(percent: int, message: str) -> None:
    """Machine-readable progress for VoxType host (stdout)."""
    _configure_stdio_utf8()
    pct = max(0, min(100, int(percent)))
    print(f"{PROGRESS_MARKER}\t{pct}\t{message}", flush=True)


def _identity_file_candidates() -> list[Path]:
    plugin_root = plugin_data_root()
    candidates = [
        sensevoice_identity_path(),
        plugin_root / "runtime" / "models" / "sensevoice-model-identity.json",
        plugin_root / "models" / "sensevoice-model-identity.json",
    ]
    seen: set[Path] = set()
    ordered: list[Path] = []
    for path in candidates:
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            ordered.append(resolved)
    return ordered


def load_sensevoice_identity() -> dict[str, Any]:
    for path in _identity_file_candidates():
        if path.is_file():
            return json.loads(path.read_text(encoding="utf-8"))
    tried = ", ".join(str(path) for path in _identity_file_candidates())
    raise RuntimeError(f"Missing model identity file (tried: {tried})")


def package_root() -> Path:
    return plugin_data_root()


def resolve_preset(name: str | None = None) -> str:
    raw = (name or os.environ.get("VOXTYPE_ASR_MODEL") or DEFAULT_PRESET).strip().lower()
    if raw in MODEL_PRESETS:
        return raw
    if raw in {"sensevoice", "sense-voice", "sense_voice"}:
        return "sensevoice"
    if raw in {"paraformer", "paraformer-zh"}:
        return "paraformer"
    return DEFAULT_PRESET


def target_dir(root: Path | None = None, preset: str | None = None) -> Path:
    base = root or package_root()
    key = resolve_preset(preset)
    return base / "models" / MODEL_PRESETS[key]["dir"]


def _model_file(dest: Path) -> Path | None:
    for name in MODEL_FILE_CANDIDATES:
        path = dest / name
        if path.is_file():
            return path
    return None


def sha256_hex_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sensevoice_files(dest: Path) -> None:
    identity = load_sensevoice_identity()
    expected_files: dict[str, Any] = identity["files"]
    for name, spec in expected_files.items():
        path = dest / name
        if not path.is_file():
            raise RuntimeError(f"Missing expected model file: {name}")
        actual_size = path.stat().st_size
        expected_size = int(spec["size"])
        if actual_size != expected_size:
            raise RuntimeError(
                f"{name} size mismatch: got {actual_size}, expected {expected_size} "
                f"({identity['id']})"
            )
        actual_hash = sha256_hex_file(path)
        expected_hash = str(spec["sha256"]).lower()
        if actual_hash != expected_hash:
            raise RuntimeError(
                f"{name} sha256 mismatch for {identity['id']}: got {actual_hash}, expected {expected_hash}"
            )


def is_model_ready(dest: Path | None = None, *, preset: str | None = None) -> bool:
    return describe_model_status(dest, preset=preset)[0]


def describe_model_status(
    dest: Path | None = None,
    *,
    preset: str | None = None,
) -> tuple[bool, str | None]:
    """Return (ready, error_message). None error means model is valid."""
    path = dest or target_dir(preset=preset)
    key = resolve_preset(preset)
    if not path.exists():
        return False, "模型目录不存在"
    if key == "sensevoice":
        try:
            verify_sensevoice_files(path)
            return True, None
        except RuntimeError as exc:
            return False, str(exc)
    missing = [name for name in REQUIRED_FILES if not (path / name).is_file()]
    if missing:
        return False, f"缺少模型文件: {', '.join(missing)}"
    tokens_path = path / "tokens.txt"
    if tokens_path.stat().st_size < PARAFORMER_MIN_TOKENS_BYTES:
        return False, "tokens.txt 文件不完整（体积过小）"
    onnx = _model_file(path)
    if onnx is None:
        return False, "缺少 ONNX 模型文件"
    if onnx.stat().st_size < PARAFORMER_MIN_ONNX_BYTES:
        return False, (
            f"{onnx.name} 文件不完整（"
            f"{onnx.stat().st_size // (1024 * 1024)} MB，"
            f"需要至少 {PARAFORMER_MIN_ONNX_BYTES // (1024 * 1024)} MB）"
        )
    return True, None


def remove_model_dir(dest: Path) -> None:
    if dest.exists():
        shutil.rmtree(dest, ignore_errors=True)


def _prepare_model_destination(
    dest: Path,
    *,
    preset: str | None,
    force: bool,
) -> None:
    ready, _ = describe_model_status(dest, preset=preset)
    if ready and not force:
        return
    if dest.exists():
        report_download_progress(1, "检测到不完整模型，正在清理…")
        remove_model_dir(dest)


def expand_download_urls(url: str) -> list[str]:
    """Prefer domestic-friendly mirrors, then the canonical GitHub URL."""
    canonical = url.strip()
    if not canonical:
        return []

    mirrors: list[str] = []
    if canonical.startswith("https://github.com/"):
        mirrors.extend(
            [
                f"https://ghfast.top/{canonical}",
                f"https://gh-proxy.com/{canonical}",
            ]
        )
    mirrors.append(canonical)

    seen: set[str] = set()
    ordered: list[str] = []
    for candidate in mirrors:
        if candidate not in seen:
            seen.add(candidate)
            ordered.append(candidate)
    return ordered


def download_file(
    url: str,
    dest: Path,
    *,
    percent_start: int = 8,
    percent_end: int = 82,
) -> None:
    last_error: Exception | None = None
    for candidate in expand_download_urls(url):
        try:
            _download_file_once(
                candidate,
                dest,
                percent_start=percent_start,
                percent_end=percent_end,
            )
            return
        except Exception as exc:  # noqa: BLE001 — try next mirror
            last_error = exc
            dest.unlink(missing_ok=True)
            print(f"  mirror failed: {exc}", file=sys.stderr)
    if last_error is None:
        raise RuntimeError(f"No download URL resolved for {url}")
    raise RuntimeError(f"All download mirrors failed for {url}: {last_error}") from last_error


def download_archive(url: str, dest: Path) -> None:
    download_file(url, dest)


def _download_file_once(
    url: str,
    dest: Path,
    *,
    percent_start: int = 8,
    percent_end: int = 82,
) -> None:
    print(f"Downloading {url}")
    print(f"  -> {dest}")
    report_download_progress(percent_start, "正在连接下载源…")
    with urllib.request.urlopen(url, timeout=300) as response:
        total = int(response.headers.get("Content-Length") or 0)
        downloaded = 0
        block = 1024 * 1024
        span = max(1, percent_end - percent_start)
        with dest.open("wb") as out:
            while True:
                chunk = response.read(block)
                if not chunk:
                    break
                out.write(chunk)
                downloaded += len(chunk)
                if total > 0:
                    ratio = downloaded / total
                    pct = percent_start + int(ratio * span)
                    mb_done = downloaded // (1024 * 1024)
                    mb_total = total // (1024 * 1024)
                    report_download_progress(
                        pct,
                        f"正在下载… {mb_done} / {mb_total} MB",
                    )
                else:
                    mb_done = downloaded // (1024 * 1024)
                    report_download_progress(
                        percent_start + span // 2,
                        f"正在下载… {mb_done} MB",
                    )
    report_download_progress(percent_end, "下载完成，准备解压…")


def download_sensevoice_from_modelscope(dest: Path) -> None:
    identity = load_sensevoice_identity()
    modelscope_base = (
        identity.get("modelscopeResolveBase")
        or identity.get("modelscope")
        or SENSEVOICE_MODELSCOPE_RESOLVE
    )
    if not str(modelscope_base).endswith("/resolve/master"):
        modelscope_base = f"{str(modelscope_base).rstrip('/')}/resolve/master"
    dest.mkdir(parents=True, exist_ok=True)
    print(f"Fetching {identity['label']} from ModelScope ({identity['id']})")
    files: dict[str, Any] = identity["files"]
    total_bytes = sum(int(spec["size"]) for spec in files.values()) or 1
    done_bytes = 0
    report_download_progress(5, f"从 ModelScope 下载 {identity['id']}…")
    for name, spec in files.items():
        out_path = dest / name
        url = f"{modelscope_base}/{name}"
        file_size = int(spec["size"])
        start = 8 + int((done_bytes / total_bytes) * 74)
        end = 8 + int(((done_bytes + file_size) / total_bytes) * 74)
        download_file(url, out_path, percent_start=start, percent_end=end)
        done_bytes += file_size
    verify_sensevoice_files(dest)


def extract_model(archive: Path, dest: Path) -> None:
    report_download_progress(86, "正在解压模型文件…")
    dest.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, mode="r:bz2") as tar:
        members = tar.getmembers()
        top_dirs = {
            m.name.split("/", maxsplit=1)[0]
            for m in members
            if "/" in m.name
        }
        if len(top_dirs) != 1:
            raise RuntimeError(f"Unexpected archive layout: {top_dirs}")
        prefix = f"{next(iter(top_dirs))}/"
        names_to_copy = [*REQUIRED_FILES, *MODEL_FILE_CANDIDATES, *OPTIONAL_FILES]
        for name in names_to_copy:
            member_name = prefix + name
            try:
                member = tar.getmember(member_name)
            except KeyError:
                continue
            extracted = tar.extractfile(member)
            if extracted is None:
                continue
            out_path = dest / name
            with out_path.open("wb") as out:
                shutil.copyfileobj(extracted, out)
            print(f"  wrote {out_path}")


def download_sensevoice_from_archive(dest: Path) -> None:
    identity = load_sensevoice_identity()
    archive_url = str(identity.get("upstream") or SENSEVOICE_ARCHIVE_URL)
    print(f"Fetching {identity['label']} from k2-fsa archive ({identity['id']})")
    archive_name = archive_url.rsplit("/", maxsplit=1)[-1]
    with tempfile.TemporaryDirectory(prefix="voxtype-model-") as tmp:
        archive = Path(tmp) / archive_name
        download_archive(archive_url, archive)
        extract_model(archive, dest)
    verify_sensevoice_files(dest)


def ensure_sensevoice_model(root: Path | None = None, *, force: bool = False) -> Path:
    dest = target_dir(root, "sensevoice")
    if is_model_ready(dest, preset="sensevoice") and not force:
        report_download_progress(100, "模型已存在")
        return dest

    _prepare_model_destination(dest, preset="sensevoice", force=force)
    report_download_progress(2, "准备下载 SenseVoice 模型…")
    errors: list[str] = []
    for fetch in (download_sensevoice_from_modelscope, download_sensevoice_from_archive):
        if dest.exists():
            shutil.rmtree(dest, ignore_errors=True)
        try:
            fetch(dest)
            report_download_progress(98, "校验模型文件…")
            report_download_progress(100, "模型下载完成")
            return dest
        except Exception as exc:  # noqa: BLE001 — try next source
            errors.append(f"{fetch.__name__}: {exc}")
            print(f"  source failed: {exc}", file=sys.stderr)

    raise RuntimeError(
        f"SenseVoice download failed ({load_sensevoice_identity()['id']}): {' | '.join(errors)}"
    )


def ensure_asr_model(
    root: Path | None = None,
    preset: str | None = None,
    *,
    force: bool = False,
) -> Path:
    key = resolve_preset(preset)
    if key == "sensevoice":
        return ensure_sensevoice_model(root, force=force)

    preset_info = MODEL_PRESETS[key]
    dest = target_dir(root, key)
    if is_model_ready(dest, preset=key) and not force:
        report_download_progress(100, "模型已存在")
        return dest

    _prepare_model_destination(dest, preset=key, force=force)
    report_download_progress(2, f"准备下载 {preset_info['label']}…")
    print(f"Fetching {preset_info['label']}")
    archive_name = preset_info["url"].rsplit("/", maxsplit=1)[-1]
    with tempfile.TemporaryDirectory(prefix="voxtype-model-") as tmp:
        archive = Path(tmp) / archive_name
        download_archive(preset_info["url"], archive)
        extract_model(archive, dest)

    if not is_model_ready(dest, preset=key):
        raise RuntimeError(f"Model files missing after extract: {dest}")
    report_download_progress(98, "校验模型文件…")
    report_download_progress(100, "模型下载完成")
    return dest


def ensure_paraformer_model(root: Path | None = None) -> Path:
    return ensure_asr_model(root, preset="paraformer")


def check_main(argv: list[str] | None = None) -> None:
    """CLI: exit 0 when model is valid, 1 with stderr message otherwise."""
    _configure_stdio_utf8()
    import argparse
    import sys

    if argv is None:
        argv = sys.argv[1:]

    parser = argparse.ArgumentParser(description="Check VoxType ASR model integrity")
    parser.add_argument("--root", type=Path, default=None, help="Plugin data root")
    parser.add_argument("--preset", default=None, help="sensevoice | paraformer")
    args = parser.parse_args(argv)
    dest = target_dir(args.root, args.preset)
    ready, err = describe_model_status(dest, preset=args.preset)
    if ready:
        print("ok")
        raise SystemExit(0)
    print(err or "model not ready", file=sys.stderr)
    raise SystemExit(1)


def main(argv: list[str] | None = None) -> None:
    _configure_stdio_utf8()
    import argparse
    import sys

    if argv is None:
        argv = sys.argv[1:]

    parser = argparse.ArgumentParser(description="Download VoxType ASR model")
    parser.add_argument("--force", action="store_true", help="Remove existing model and re-download")
    parser.add_argument("--preset", default=None, help="sensevoice | paraformer")
    parser.add_argument("--root", type=Path, default=None, help="Plugin data root")
    args, _unknown = parser.parse_known_args(argv)

    preset = resolve_preset(args.preset)
    try:
        path = ensure_asr_model(args.root, preset=preset, force=args.force)
    except Exception as exc:
        print(f"Download failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    model_file = _model_file(path)
    if preset == "sensevoice" or resolve_preset(preset) == "sensevoice":
        identity = load_sensevoice_identity()
        print(f"Verified {identity['id']}")
    print(f"ASR model ready at {path} ({model_file.name if model_file else '?'})")


if __name__ == "__main__":
    main()
