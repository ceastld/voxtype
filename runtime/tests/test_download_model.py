from __future__ import annotations

from pathlib import Path

from voxtype_runtime.download_model import (
    describe_model_status,
    load_sensevoice_identity,
    remove_model_dir,
    target_dir,
)


def test_describe_model_status_missing_dir(tmp_path: Path) -> None:
    dest = target_dir(tmp_path, "sensevoice")
    ready, err = describe_model_status(dest, preset="sensevoice")
    assert ready is False
    assert err is not None


def test_describe_model_status_partial_onnx(tmp_path: Path) -> None:
    dest = target_dir(tmp_path, "paraformer")
    dest.mkdir(parents=True)
    (dest / "tokens.txt").write_text("a", encoding="utf-8")
    (dest / "model.onnx").write_bytes(b"x" * 512)
    ready, err = describe_model_status(dest, preset="paraformer")
    assert ready is False
    assert err is not None
    assert err is not None
    assert "不完�? in err or "过小" in err


def test_load_sensevoice_identity_from_plugin_runtime_fallback(tmp_path: Path, monkeypatch) -> None:
    identity_src = Path(__file__).resolve().parents[1] / "models" / "sensevoice-model-identity.json"
    runtime_models = tmp_path / "runtime" / "models"
    runtime_models.mkdir(parents=True)
    fallback = runtime_models / "sensevoice-model-identity.json"
    fallback.write_text(identity_src.read_text(encoding="utf-8"), encoding="utf-8")

    monkeypatch.setenv("VOXTYPE_PLUGIN_ROOT", str(tmp_path))
    monkeypatch.setattr(
        "voxtype_runtime.download_model.sensevoice_identity_path",
        lambda: tmp_path / "missing-bundled" / "sensevoice-model-identity.json",
    )

    identity = load_sensevoice_identity()
    assert identity["id"] == "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17"


def test_remove_model_dir_clears_partial(tmp_path: Path) -> None:
    dest = target_dir(tmp_path, "paraformer")
    dest.mkdir(parents=True)
    (dest / "tokens.txt").write_text("a", encoding="utf-8")
    remove_model_dir(dest)
    assert not dest.exists()
