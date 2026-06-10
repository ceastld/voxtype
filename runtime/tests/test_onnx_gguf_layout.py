from __future__ import annotations

from pathlib import Path

from voxtype_runtime.recognizer.onnx_gguf_layout import (
    has_funasr_hybrid_layout,
    has_qwen_hybrid_layout,
    resolve_funasr_hybrid_paths,
    resolve_qwen_hybrid_paths,
)


def test_funasr_hybrid_layout(tmp_path: Path) -> None:
    model_dir = tmp_path / "fun-asr-nano"
    model_dir.mkdir()
    (model_dir / "Fun-ASR-Nano-Encoder-Adaptor.int4.onnx").write_bytes(b"x")
    (model_dir / "Fun-ASR-Nano-CTC.int4.onnx").write_bytes(b"x")
    (model_dir / "Fun-ASR-Nano-Decoder.q5_k.gguf").write_bytes(b"x")
    (model_dir / "tokens.txt").write_text("tok", encoding="utf-8")
    assert has_funasr_hybrid_layout(model_dir)
    paths = resolve_funasr_hybrid_paths(model_dir)
    assert paths is not None
    assert paths.decoder_gguf.suffix == ".gguf"


def test_qwen_hybrid_layout(tmp_path: Path) -> None:
    model_dir = tmp_path / "qwen-asr"
    model_dir.mkdir()
    (model_dir / "qwen3_asr_encoder_frontend.fp16.onnx").write_bytes(b"x")
    (model_dir / "qwen3_asr_encoder_backend.fp16.onnx").write_bytes(b"x")
    (model_dir / "qwen3_asr_llm.q5_k.gguf").write_bytes(b"x")
    assert has_qwen_hybrid_layout(model_dir)
    paths = resolve_qwen_hybrid_paths(model_dir)
    assert paths is not None
