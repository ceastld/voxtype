from __future__ import annotations

import logging
import os
from pathlib import Path

from voxtype_runtime.recognizer.base import Recognizer
from voxtype_runtime.recognizer.stub import StubRecognizer

logger = logging.getLogger(__name__)


def _engine_backend() -> str:
    return os.environ.get("VOXTYPE_ENGINE_BACKEND", "onnx_gguf").strip().lower()


def create_recognizer(
    model_dir: Path | None,
    model_type: str | None,
    *,
    provider: str = "cpu",
    num_threads: int = 4,
) -> Recognizer:
    if model_dir is not None:
        backend = _engine_backend()
        if backend != "sherpa_onnx":
            from voxtype_runtime.recognizer.onnx_gguf import try_create_onnx_gguf_recognizer

            hybrid = try_create_onnx_gguf_recognizer(
                model_dir,
                model_type,
                provider=provider,
                num_threads=num_threads,
            )
            if hybrid is not None:
                logger.info("Loaded ONNX+GGUF recognizer from %s", model_dir)
                return hybrid
            if backend == "onnx_gguf":
                logger.warning(
                    "ONNX+GGUF layout missing under %s; falling back to sherpa-onnx",
                    model_dir,
                )

        from voxtype_runtime.recognizer.sherpa_onnx import try_create_sherpa_recognizer

        sherpa = try_create_sherpa_recognizer(
            model_dir,
            model_type,
            provider=provider,
            num_threads=num_threads,
        )
        if sherpa is not None:
            logger.info("Loaded sherpa-onnx recognizer from %s", model_dir)
            return sherpa
        logger.warning(
            "Model dir %s present but no recognizer backend succeeded; using stub",
            model_dir,
        )

    sensevoice_dir = Path(__file__).resolve().parents[3] / "models" / "sensevoice"
    if sensevoice_dir.is_dir() and model_dir is None:
        from voxtype_runtime.recognizer.sherpa_onnx import try_create_sherpa_recognizer

        sherpa = try_create_sherpa_recognizer(
            sensevoice_dir,
            model_type or "sensevoice",
            provider=provider,
            num_threads=num_threads,
        )
        if sherpa is not None:
            logger.info("Loaded sherpa-onnx recognizer from %s", sensevoice_dir)
            return sherpa

    paraformer_dir = Path(__file__).resolve().parents[3] / "models" / "paraformer-zh"
    if paraformer_dir.is_dir() and model_dir is None:
        from voxtype_runtime.recognizer.sherpa_onnx import try_create_sherpa_recognizer

        sherpa = try_create_sherpa_recognizer(
            paraformer_dir,
            model_type or "paraformer",
            provider=provider,
            num_threads=num_threads,
        )
        if sherpa is not None:
            logger.info("Loaded sherpa-onnx recognizer from %s", paraformer_dir)
            return sherpa

    logger.info("Using stub recognizer (no ASR model or sherpa-onnx unavailable)")
    return StubRecognizer()
