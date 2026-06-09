from __future__ import annotations

import logging
import struct
from pathlib import Path

import sherpa_onnx

from voxtype_runtime.recognizer.base import Recognizer

logger = logging.getLogger(__name__)

_VALID_PROVIDERS = frozenset({"cpu", "cuda", "directml", "coreml", "trt"})


def _pcm_s16le_to_float32(pcm: bytes) -> list[float]:
    count = len(pcm) // 2
    if count == 0:
        return []
    samples = struct.unpack(f"<{count}h", pcm[: count * 2])
    return [s / 32768.0 for s in samples]


def _find_onnx(model_dir: Path) -> Path | None:
    preferred = (
        model_dir / "model.int8.onnx",
        model_dir / "model.onnx",
        model_dir / "encoder.int8.onnx",
        model_dir / "encoder.onnx",
    )
    for path in preferred:
        if path.is_file():
            return path
    matches = sorted(model_dir.glob("*.onnx"))
    return matches[0] if matches else None


def _detect_model_type(model_dir: Path, explicit: str | None) -> str | None:
    if explicit:
        return explicit
    name = model_dir.name.lower()
    if "sensevoice" in name or "sense_voice" in name or "sense-voice" in name:
        return "sensevoice"
    if "paraformer" in name:
        return "paraformer"
    if "whisper" in name:
        return "whisper"
    if (model_dir / "model.int8.onnx").is_file() or (model_dir / "model.onnx").is_file():
        if "sensevoice" in name or "sense_voice" in name or "sense-voice" in name:
            return "sensevoice"
        if "paraformer" in name:
            return "paraformer"
        if (model_dir / "model.int8.onnx").is_file():
            return "sensevoice"
        return "paraformer"
    if (model_dir / "encoder.onnx").is_file() or (
        model_dir / "encoder.int8.onnx"
    ).is_file():
        return "paraformer"
    return None


def _normalize_provider(provider: str | None) -> str:
    raw = (provider or "cpu").strip().lower()
    return raw if raw in _VALID_PROVIDERS else "cpu"


def _provider_chain(requested: str) -> list[str]:
    normalized = _normalize_provider(requested)
    if normalized == "cpu":
        return ["cpu"]
    return [normalized, "cpu"]


def _build_offline_recognizer(
    kind: str,
    *,
    onnx_path: Path,
    tokens: Path,
    model_dir: Path,
    provider: str,
    num_threads: int,
) -> sherpa_onnx.OfflineRecognizer:
    if kind == "sensevoice":
        return sherpa_onnx.OfflineRecognizer.from_sense_voice(
            model=str(onnx_path),
            tokens=str(tokens),
            num_threads=num_threads,
            debug=False,
            provider=provider,
            language="auto",
            use_itn=True,
        )
    if kind == "paraformer":
        return sherpa_onnx.OfflineRecognizer.from_paraformer(
            paraformer=str(onnx_path),
            tokens=str(tokens),
            num_threads=num_threads,
            debug=False,
            provider=provider,
        )
    if kind == "whisper":
        decoder = model_dir / "decoder.onnx"
        if not decoder.is_file():
            decoder = model_dir / "decoder.int8.onnx"
        if not decoder.is_file():
            raise FileNotFoundError(f"Missing whisper decoder in {model_dir}")
        return sherpa_onnx.OfflineRecognizer.from_whisper(
            encoder=str(onnx_path),
            decoder=str(decoder),
            tokens=str(tokens),
            num_threads=num_threads,
            debug=False,
            provider=provider,
        )
    raise ValueError(f"Unsupported model kind: {kind}")


def try_create_sherpa_recognizer(
    model_dir: Path,
    model_type: str | None,
    *,
    provider: str = "cpu",
    num_threads: int = 4,
) -> Recognizer | None:
    if not model_dir.is_dir():
        return None

    tokens = model_dir / "tokens.txt"
    if not tokens.is_file():
        logger.warning("Missing tokens.txt in %s", model_dir)
        return None

    onnx_path = _find_onnx(model_dir)
    if onnx_path is None:
        logger.warning("No .onnx model in %s", model_dir)
        return None

    kind = _detect_model_type(model_dir, model_type)
    if kind is None:
        logger.warning("Could not detect model type in %s", model_dir)
        return None

    threads = max(1, min(int(num_threads), 32))
    last_error: Exception | None = None

    for prov in _provider_chain(provider):
        try:
            recognizer = _build_offline_recognizer(
                kind,
                onnx_path=onnx_path,
                tokens=tokens,
                model_dir=model_dir,
                provider=prov,
                num_threads=threads,
            )
            if prov != _normalize_provider(provider):
                logger.warning(
                    "Requested provider %s unavailable; fell back to %s",
                    provider,
                    prov,
                )
            else:
                logger.info("Sherpa recognizer using provider=%s threads=%s", prov, threads)
            return _SherpaOnnxRecognizer(
                recognizer=recognizer,
                model_id=kind,
                execution_provider=prov,
            )
        except Exception as exc:
            last_error = exc
            logger.warning("Sherpa provider %s failed for %s: %s", prov, model_dir, exc)

    if last_error is not None:
        logger.exception("Failed to create sherpa-onnx recognizer from %s", model_dir)
    return None


class _SherpaOnnxRecognizer:
    def __init__(
        self,
        recognizer: sherpa_onnx.OfflineRecognizer,
        model_id: str,
        execution_provider: str,
    ) -> None:
        self._recognizer = recognizer
        self.model_id = model_id
        self.execution_provider = execution_provider
        self.ready = True

    def transcribe(self, pcm_s16le: bytes, *, sample_rate: int, language: str) -> str:
        del language  # SenseVoice auto-detects; per-request override not needed in v1
        samples = _pcm_s16le_to_float32(pcm_s16le)
        if not samples:
            return ""
        import numpy as np

        stream = self._recognizer.create_stream()
        stream.accept_waveform(sample_rate, np.array(samples, dtype=np.float32))
        self._recognizer.decode_stream(stream)
        return (stream.result.text or "").strip()
