from __future__ import annotations

import argparse
import logging
import os
from dataclasses import dataclass
from pathlib import Path

from voxtype_runtime.paths import default_models_dir


def _default_provider() -> str:
    raw = os.environ.get("VOXTYPE_PROVIDER", "cpu").strip().lower()
    if raw in {"cpu", "cuda", "directml", "coreml", "trt"}:
        return raw
    return "cpu"


def _default_num_threads() -> int:
    raw = os.environ.get("VOXTYPE_NUM_THREADS", "4").strip()
    try:
        value = int(raw)
    except ValueError:
        return 4
    return max(1, min(value, 32))


@dataclass(frozen=True)
class RuntimeConfig:
    host: str
    port: int
    transport: str
    model_dir: Path | None
    model_type: str | None
    provider: str
    num_threads: int
    log_level: str

    @property
    def runtime_version(self) -> str:
        from voxtype_runtime import __version__

        return __version__


def _default_model_dir() -> Path:
    return default_models_dir()


def load_config(argv: list[str] | None = None) -> RuntimeConfig:
    parser = argparse.ArgumentParser(
        prog="voxtype-runtime",
        description="VoxType local ASR server (voxtype-voice-v1)",
    )
    parser.add_argument(
        "--host",
        default=os.environ.get("VOXTYPE_HOST", "127.0.0.1"),
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("VOXTYPE_PORT", "6016")),
    )
    parser.add_argument(
        "--transport",
        choices=("tcp", "stdio"),
        default=os.environ.get("VOXTYPE_TRANSPORT", "tcp"),
        help="tcp: HTTP+WebSocket; stdio: framed JSON/PCM on stdin/stdout (Tauri IPC)",
    )
    parser.add_argument(
        "--model-dir",
        default=os.environ.get("VOXTYPE_MODEL_DIR"),
        help="Directory with sherpa-onnx model files (optional)",
    )
    parser.add_argument(
        "--model-type",
        default=os.environ.get("VOXTYPE_MODEL_TYPE"),
        help="sherpa model family: sensevoice | paraformer | fun_asr_nano | qwen_asr | whisper",
    )
    parser.add_argument(
        "--log-level",
        default=os.environ.get("VOXTYPE_LOG_LEVEL", "INFO"),
    )
    parser.add_argument(
        "--provider",
        default=os.environ.get("VOXTYPE_PROVIDER", "cpu"),
        help="ONNX execution provider: cpu | directml | cuda | coreml",
    )
    parser.add_argument(
        "--num-threads",
        type=int,
        default=int(os.environ.get("VOXTYPE_NUM_THREADS", "4")),
        help="CPU thread count when provider is cpu",
    )
    args = parser.parse_args(argv)

    model_dir: Path | None = None
    if args.model_dir:
        model_dir = Path(args.model_dir).expanduser().resolve()
    else:
        candidate = _default_model_dir()
        sensevoice = candidate / "sensevoice"
        paraformer = candidate / "paraformer-zh"
        if (sensevoice / "tokens.txt").is_file() and (
            (sensevoice / "model.int8.onnx").is_file() or (sensevoice / "model.onnx").is_file()
        ):
            model_dir = sensevoice.resolve()
        elif (paraformer / "tokens.txt").is_file() and (
            (paraformer / "model.int8.onnx").is_file() or (paraformer / "model.onnx").is_file()
        ):
            model_dir = paraformer.resolve()
        elif candidate.is_dir():
            entries = [
                p
                for p in candidate.iterdir()
                if p.name not in {".gitkeep", "README.md"}
            ]
            subs = [p for p in entries if p.is_dir()]
            if len(subs) == 1:
                model_dir = subs[0].resolve()
            elif (candidate / "tokens.txt").is_file() or list(candidate.glob("*.onnx")):
                model_dir = candidate.resolve()

    provider = str(args.provider).strip().lower()
    if provider not in {"cpu", "cuda", "directml", "coreml", "trt"}:
        provider = _default_provider()

    num_threads = max(1, min(int(args.num_threads), 32))

    return RuntimeConfig(
        host=str(args.host),
        port=int(args.port),
        transport=str(args.transport),
        model_dir=model_dir,
        model_type=str(args.model_type) if args.model_type else None,
        provider=provider,
        num_threads=num_threads,
        log_level=str(args.log_level).upper(),
    )


def configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level, logging.INFO),
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
