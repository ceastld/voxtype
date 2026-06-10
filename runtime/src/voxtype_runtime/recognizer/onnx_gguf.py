from __future__ import annotations

import logging
from pathlib import Path

import numpy as np

from voxtype_runtime.recognizer.base import Recognizer
from voxtype_runtime.recognizer.onnx_gguf_layout import (
    has_funasr_hybrid_layout,
    has_qwen_hybrid_layout,
    resolve_funasr_hybrid_paths,
    resolve_qwen_hybrid_paths,
)
from voxtype_runtime.recognizer.sherpa_onnx import _pcm_s16le_to_float32

logger = logging.getLogger(__name__)


def _map_onnx_provider(provider: str) -> str:
    raw = (provider or "cpu").strip().lower()
    if raw == "cuda":
        return "CUDA"
    if raw in {"directml", "dml"}:
        return "DML"
    return "CPU"


def _llm_use_gpu(provider: str) -> bool:
    return (provider or "cpu").strip().lower() in {"cuda", "directml", "dml", "coreml"}


def _ensure_third_party() -> bool:
    try:
        from voxtype_runtime.recognizer.third_party_bootstrap import ensure_third_party_paths

        roots = ensure_third_party_paths()
        return bool(roots)
    except Exception as exc:
        logger.warning("third_party bootstrap failed: %s", exc)
        return False


def _detect_hybrid_kind(model_dir: Path, model_type: str | None) -> str | None:
    explicit = (model_type or "").strip().lower().replace("-", "_")
    if explicit in {"fun_asr_nano", "funasr_nano"}:
        return "fun_asr_nano" if has_funasr_hybrid_layout(model_dir) else None
    if explicit in {"qwen_asr", "qwen3_asr"}:
        return "qwen_asr" if has_qwen_hybrid_layout(model_dir) else None
    if has_funasr_hybrid_layout(model_dir):
        return "fun_asr_nano"
    if has_qwen_hybrid_layout(model_dir):
        return "qwen_asr"
    return None


class _FunAsrGgufRecognizer:
    def __init__(self, engine: object, *, provider: str) -> None:
        self._engine = engine
        self.model_id = "fun_asr_nano"
        self.execution_provider = provider
        self.ready = True

    def transcribe(self, pcm_s16le: bytes, *, sample_rate: int, language: str) -> str:
        samples = _pcm_s16le_to_float32(pcm_s16le)
        if not samples:
            return ""
        stream = self._engine.create_stream()
        stream.accept_waveform(sample_rate, np.array(samples, dtype=np.float32))
        lang = language.strip() or None
        result = self._engine.decode_stream(
            stream,
            language=lang,
            verbose=False,
        )
        return (getattr(result, "text", "") or "").strip()


def _map_qwen_language(language: str) -> str | None:
    raw = (language or "").strip().lower()
    if not raw or raw in {"auto", "automatic"}:
        return None

    # BCP-47 locale tags: zh-cn, en-us -> primary subtag
    primary = raw.replace("_", "-").split("-", 1)[0]

    mapping = {
        "zh": "Chinese",
        "cn": "Chinese",
        "chinese": "Chinese",
        "mandarin": "Chinese",
        "en": "English",
        "english": "English",
        "ja": "Japanese",
        "jp": "Japanese",
        "japanese": "Japanese",
        "yue": "Cantonese",
        "cantonese": "Cantonese",
        "ko": "Korean",
        "kr": "Korean",
        "korean": "Korean",
        "de": "German",
        "fr": "French",
        "es": "Spanish",
        "pt": "Portuguese",
        "id": "Indonesian",
        "it": "Italian",
        "ru": "Russian",
        "th": "Thai",
        "vi": "Vietnamese",
        "tr": "Turkish",
        "hi": "Hindi",
        "ms": "Malay",
        "nl": "Dutch",
        "sv": "Swedish",
        "da": "Danish",
        "fi": "Finnish",
        "pl": "Polish",
        "cs": "Czech",
        "fil": "Filipino",
        "fa": "Persian",
        "el": "Greek",
        "ro": "Romanian",
        "hu": "Hungarian",
        "mk": "Macedonian",
        "ar": "Arabic",
    }

    if primary in mapping:
        return mapping[primary]
    if raw in mapping:
        return mapping[raw]

    supported = {
        "chinese",
        "english",
        "cantonese",
        "arabic",
        "german",
        "french",
        "spanish",
        "portuguese",
        "indonesian",
        "italian",
        "korean",
        "russian",
        "thai",
        "vietnamese",
        "japanese",
        "turkish",
        "hindi",
        "malay",
        "dutch",
        "swedish",
        "danish",
        "finnish",
        "polish",
        "czech",
        "filipino",
        "persian",
        "greek",
        "romanian",
        "hungarian",
        "macedonian",
    }
    if raw in supported:
        return raw.capitalize() if len(raw) == len(raw.lower()) else language.strip()

    # Unknown tag: let Qwen auto-detect instead of failing validation.
    return None


class _QwenGgufRecognizer:
    def __init__(self, engine: object, *, provider: str) -> None:
        self._engine = engine
        self.model_id = "qwen_asr"
        self.execution_provider = provider
        self.ready = True

    def transcribe(self, pcm_s16le: bytes, *, sample_rate: int, language: str) -> str:
        samples = _pcm_s16le_to_float32(pcm_s16le)
        if not samples:
            return ""
        lang = _map_qwen_language(language)
        duration_sec = len(samples) / max(sample_rate, 1)
        # Qwen defaults to 40s chunks and zero-pads short audio to 40s, which hurts
        # short hotkey dictation (slow + less accurate). Scale chunk to utterance length.
        chunk_size_sec = min(40.0, max(1.0, duration_sec + 0.5))
        result = self._engine.asr(
            audio=np.array(samples, dtype=np.float32),
            context="",
            language=lang,
            chunk_size_sec=chunk_size_sec,
            memory_chunks=1,
        )
        return (getattr(result, "text", "") or "").strip()


def try_create_onnx_gguf_recognizer(
    model_dir: Path,
    model_type: str | None,
    *,
    provider: str = "cpu",
    num_threads: int = 4,
) -> Recognizer | None:
    if not _ensure_third_party():
        return None

    kind = _detect_hybrid_kind(model_dir, model_type)
    if kind is None:
        return None

    onnx_provider = _map_onnx_provider(provider)
    use_gpu = _llm_use_gpu(provider)
    threads = max(2, min(int(num_threads), 32))

    try:
        if kind == "fun_asr_nano":
            paths = resolve_funasr_hybrid_paths(model_dir)
            if paths is None:
                return None
            from fun_asr_gguf import ASREngineConfig, FunASREngine

            config = ASREngineConfig(
                encoder_onnx_path=str(paths.encoder_onnx),
                ctc_onnx_path=str(paths.ctc_onnx),
                decoder_gguf_path=str(paths.decoder_gguf),
                tokens_path=str(paths.tokens),
                enable_ctc=True,
                n_threads=threads,
                onnx_provider=onnx_provider,
                llm_use_gpu=use_gpu,
                verbose=False,
            )
            engine = FunASREngine(config)
            logger.info(
                "Loaded FunASR ONNX+GGUF from %s (onnx=%s llm_gpu=%s)",
                model_dir,
                onnx_provider,
                use_gpu,
            )
            return _FunAsrGgufRecognizer(engine, provider=onnx_provider.lower())

        paths = resolve_qwen_hybrid_paths(model_dir)
        if paths is None:
            return None
        from qwen_asr_gguf.inference.asr import QwenASREngine
        from qwen_asr_gguf.inference.schema import ASREngineConfig

        config = ASREngineConfig(
            model_dir=str(model_dir),
            encoder_frontend_fn=paths.encoder_frontend.name,
            encoder_backend_fn=paths.encoder_backend.name,
            llm_fn=paths.decoder_gguf.name,
            onnx_provider=onnx_provider,
            llm_use_gpu=use_gpu,
            verbose=False,
            enable_aligner=False,
        )
        engine = QwenASREngine(config)
        logger.info(
            "Loaded Qwen3-ASR ONNX+GGUF from %s (onnx=%s llm_gpu=%s)",
            model_dir,
            onnx_provider,
            use_gpu,
        )
        return _QwenGgufRecognizer(engine, provider=onnx_provider.lower())
    except Exception as exc:
        logger.warning("ONNX+GGUF recognizer failed for %s: %s", model_dir, exc)
        return None
