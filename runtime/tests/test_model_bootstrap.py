from __future__ import annotations

from pathlib import Path

from voxtype_runtime.config import RuntimeConfig
from voxtype_runtime.model_bootstrap import resolve_runtime_model


def _base_config(model_dir: Path) -> RuntimeConfig:
    return RuntimeConfig(
        host="127.0.0.1",
        port=6016,
        transport="tcp",
        model_dir=model_dir,
        model_type=None,
        provider="cpu",
        num_threads=4,
        log_level="INFO",
    )


def test_qwen_hybrid_survives_missing_model_type(tmp_path: Path, monkeypatch) -> None:
    dest = tmp_path / "models" / "qwen-asr"
    dest.mkdir(parents=True)
    (dest / "qwen3_asr_encoder_frontend.fp16.onnx").write_bytes(b"x" * 1024)
    (dest / "qwen3_asr_encoder_backend.fp16.onnx").write_bytes(b"x" * 1024)
    (dest / "qwen3_asr_llm.q5_k.gguf").write_bytes(b"x" * 1024)

    monkeypatch.setenv("VOXTYPE_AUTO_DOWNLOAD_MODEL", "0")
    monkeypatch.setenv("VOXTYPE_PLUGIN_ROOT", str(tmp_path))

    resolved = resolve_runtime_model(_base_config(dest))
    assert resolved.model_dir == dest.resolve()
    assert resolved.model_type == "qwen_asr"
    assert dest.is_dir()


def test_invalid_partial_not_deleted_when_auto_download_disabled(
    tmp_path: Path,
    monkeypatch,
) -> None:
    dest = tmp_path / "models" / "qwen-asr"
    dest.mkdir(parents=True)
    (dest / "partial.bin").write_bytes(b"x")

    monkeypatch.setenv("VOXTYPE_AUTO_DOWNLOAD_MODEL", "0")
    monkeypatch.setenv("VOXTYPE_PLUGIN_ROOT", str(tmp_path))

    try:
        resolve_runtime_model(_base_config(dest))
    except SystemExit:
        pass

    assert dest.is_dir()
    assert (dest / "partial.bin").is_file()
