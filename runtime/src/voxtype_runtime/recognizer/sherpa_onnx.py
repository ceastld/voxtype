from __future__ import annotations

import logging
import struct
import sys
from pathlib import Path

import sherpa_onnx

from voxtype_runtime.recognizer.base import Recognizer

logger = logging.getLogger(__name__)

_VALID_PROVIDERS = frozenset({"cpu", "cuda", "directml", "coreml", "trt"})
_FUNASR_USER_PROMPT = "语音转写:"


def _pcm_s16le_to_float32(pcm: bytes) -> list[float]:
    count = len(pcm) // 2
    if count == 0:
        return []
    samples = struct.unpack(f"<{count}h", pcm[: count * 2])
    return [s / 32768.0 for s in samples]


def _pick_onnx(model_dir: Path, stem: str) -> Path | None:
    for name in (f"{stem}.int8.onnx", f"{stem}.onnx"):
        path = model_dir / name
        if path.is_file():
            return path
    return None


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


def _find_tokenizer_dir(model_dir: Path) -> Path | None:
    for name in ("tokenizer", "Qwen3-0.6B"):
        path = model_dir / name
        if not path.is_dir():
            continue
        if (path / "tokenizer.json").is_file() or (path / "vocab.json").is_file():
            return path
    return None


def _has_funasr_nano_layout(model_dir: Path) -> bool:
    return (
        _pick_onnx(model_dir, "encoder_adaptor") is not None
        and _pick_onnx(model_dir, "llm") is not None
        and _pick_onnx(model_dir, "embedding") is not None
        and _find_tokenizer_dir(model_dir) is not None
    )


def _has_qwen_asr_layout(model_dir: Path) -> bool:
    return (
        (model_dir / "conv_frontend.onnx").is_file()
        and _pick_onnx(model_dir, "encoder") is not None
        and _pick_onnx(model_dir, "decoder") is not None
        and _find_tokenizer_dir(model_dir) is not None
    )


def _detect_model_type(model_dir: Path, explicit: str | None) -> str | None:
    if explicit:
        normalized = explicit.strip().lower().replace("-", "_")
        aliases = {
            "funasr_nano": "fun_asr_nano",
            "fun_asr_nano": "fun_asr_nano",
            "qwen3_asr": "qwen_asr",
            "qwen_asr": "qwen_asr",
        }
        if normalized in aliases:
            return aliases[normalized]
        if normalized in {"sensevoice", "paraformer", "whisper"}:
            return normalized

    name = model_dir.name.lower()
    if _has_funasr_nano_layout(model_dir) or "funasr" in name or "fun-asr" in name:
        return "fun_asr_nano"
    if _has_qwen_asr_layout(model_dir) or "qwen3-asr" in name or "qwen-asr" in name:
        return "qwen_asr"
    if "sensevoice" in name or "sense_voice" in name or "sense-voice" in name:
        return "sensevoice"
    if "paraformer" in name:
        return "paraformer"
    if "whisper" in name:
        return "whisper"
    if (model_dir / "model.int8.onnx").is_file() or (model_dir / "model.onnx").is_file():
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
    # CPU sherpa wheels on Windows ship DirectML, not CUDA.
    if sys.platform == "win32" and normalized == "cuda":
        return ["directml", "cpu"]
    return [normalized, "cpu"]


def _build_offline_recognizer(
    kind: str,
    *,
    onnx_path: Path | None,
    tokens: Path | None,
    model_dir: Path,
    provider: str,
    num_threads: int,
) -> sherpa_onnx.OfflineRecognizer:
    if kind == "sensevoice":
        if onnx_path is None or tokens is None:
            raise FileNotFoundError(f"Missing sensevoice model in {model_dir}")
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
        if onnx_path is None or tokens is None:
            raise FileNotFoundError(f"Missing paraformer model in {model_dir}")
        return sherpa_onnx.OfflineRecognizer.from_paraformer(
            paraformer=str(onnx_path),
            tokens=str(tokens),
            num_threads=num_threads,
            debug=False,
            provider=provider,
        )
    if kind == "whisper":
        if onnx_path is None or tokens is None:
            raise FileNotFoundError(f"Missing whisper encoder in {model_dir}")
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
    if kind == "fun_asr_nano":
        encoder_adaptor = _pick_onnx(model_dir, "encoder_adaptor")
        llm = _pick_onnx(model_dir, "llm")
        embedding = _pick_onnx(model_dir, "embedding")
        tokenizer = _find_tokenizer_dir(model_dir)
        if encoder_adaptor is None or llm is None or embedding is None or tokenizer is None:
            raise FileNotFoundError(f"Missing FunASR-Nano files in {model_dir}")
        return sherpa_onnx.OfflineRecognizer.from_funasr_nano(
            encoder_adaptor=str(encoder_adaptor),
            llm=str(llm),
            embedding=str(embedding),
            tokenizer=str(tokenizer),
            num_threads=num_threads,
            debug=False,
            provider=provider,
            user_prompt=_FUNASR_USER_PROMPT,
            itn=True,
        )
    if kind == "qwen_asr":
        conv_frontend = model_dir / "conv_frontend.onnx"
        encoder = _pick_onnx(model_dir, "encoder")
        decoder = _pick_onnx(model_dir, "decoder")
        tokenizer = _find_tokenizer_dir(model_dir)
        if not conv_frontend.is_file() or encoder is None or decoder is None or tokenizer is None:
            raise FileNotFoundError(f"Missing Qwen3-ASR files in {model_dir}")
        return sherpa_onnx.OfflineRecognizer.from_qwen3_asr(
            conv_frontend=str(conv_frontend),
            encoder=str(encoder),
            decoder=str(decoder),
            tokenizer=str(tokenizer),
            num_threads=num_threads,
            debug=False,
            provider=provider,
            max_new_tokens=256,
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

    kind = _detect_model_type(model_dir, model_type)
    if kind is None:
        logger.warning("Could not detect model type in %s", model_dir)
        return None

    tokens: Path | None = None
    onnx_path: Path | None = None
    if kind in {"sensevoice", "paraformer", "whisper"}:
        tokens = model_dir / "tokens.txt"
        if not tokens.is_file():
            logger.warning("Missing tokens.txt in %s", model_dir)
            return None
        onnx_path = _find_onnx(model_dir)
        if onnx_path is None:
            logger.warning("No .onnx model in %s", model_dir)
            return None
    elif kind == "fun_asr_nano" and not _has_funasr_nano_layout(model_dir):
        logger.warning("Incomplete FunASR-Nano layout in %s", model_dir)
        return None
    elif kind == "qwen_asr" and not _has_qwen_asr_layout(model_dir):
        logger.warning("Incomplete Qwen3-ASR layout in %s", model_dir)
        return None

    threads = max(1, min(int(num_threads), 32))
    if kind in {"fun_asr_nano", "qwen_asr"}:
        threads = max(2, threads)

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
