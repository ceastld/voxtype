"""Detect ONNX+GGUF hybrid model layouts."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class FunAsrHybridPaths:
    encoder_onnx: Path
    ctc_onnx: Path
    decoder_gguf: Path
    tokens: Path


@dataclass(frozen=True)
class QwenHybridPaths:
    encoder_frontend: Path
    encoder_backend: Path
    decoder_gguf: Path


def _pick_glob_one(model_dir: Path, patterns: tuple[str, ...]) -> Path | None:
    for pattern in patterns:
        matches = sorted(model_dir.glob(pattern))
        if matches:
            return matches[0]
    return None


def _pick_funasr_encoder(model_dir: Path) -> Path | None:
    preferred = sorted(model_dir.glob("Fun-ASR-Nano-Encoder-Adaptor*.onnx"))
    if preferred:
        return preferred[0]
    return _pick_glob_one(
        model_dir,
        (
            "*Encoder*Adaptor*.onnx",
            "encoder_adaptor*.onnx",
        ),
    )


def _pick_funasr_ctc(model_dir: Path) -> Path | None:
    preferred = sorted(model_dir.glob("Fun-ASR-Nano-CTC*.onnx"))
    if preferred:
        return preferred[0]
    return _pick_glob_one(model_dir, ("*CTC*.onnx", "ctc*.onnx"))


def _pick_decoder_gguf(model_dir: Path, preferred: tuple[str, ...]) -> Path | None:
    hit = _pick_glob_one(model_dir, preferred)
    if hit is not None:
        return hit
    matches = sorted(model_dir.glob("*.gguf"))
    return matches[0] if matches else None


def has_funasr_hybrid_layout(model_dir: Path) -> bool:
    return resolve_funasr_hybrid_paths(model_dir) is not None


def has_qwen_hybrid_layout(model_dir: Path) -> bool:
    return resolve_qwen_hybrid_paths(model_dir) is not None


def resolve_funasr_hybrid_paths(model_dir: Path) -> FunAsrHybridPaths | None:
    if not model_dir.is_dir():
        return None
    encoder = _pick_funasr_encoder(model_dir)
    ctc = _pick_funasr_ctc(model_dir)
    decoder = _pick_decoder_gguf(
        model_dir,
        ("*Decoder*.gguf", "Fun-ASR-Nano-Decoder*.gguf", "funasr*.gguf"),
    )
    tokens = model_dir / "tokens.txt"
    if encoder is None or ctc is None or decoder is None or not tokens.is_file():
        return None
    return FunAsrHybridPaths(
        encoder_onnx=encoder,
        ctc_onnx=ctc,
        decoder_gguf=decoder,
        tokens=tokens,
    )


def resolve_qwen_hybrid_paths(model_dir: Path) -> QwenHybridPaths | None:
    if not model_dir.is_dir():
        return None
    frontend = _pick_glob_one(
        model_dir,
        (
            "qwen3_asr_encoder_frontend*.onnx",
            "*encoder_frontend*.onnx",
        ),
    )
    backend = _pick_glob_one(
        model_dir,
        (
            "qwen3_asr_encoder_backend*.onnx",
            "*encoder_backend*.onnx",
        ),
    )
    decoder = _pick_decoder_gguf(
        model_dir,
        ("qwen3_asr_llm*.gguf", "Qwen3*ASR*.gguf"),
    )
    if frontend is None or backend is None or decoder is None:
        return None
    return QwenHybridPaths(
        encoder_frontend=frontend,
        encoder_backend=backend,
        decoder_gguf=decoder,
    )
