from __future__ import annotations

from voxtype_runtime.config import load_config


def test_load_config_provider_and_threads(monkeypatch) -> None:
    monkeypatch.delenv("VOXTYPE_MODEL_DIR", raising=False)
    monkeypatch.setenv("VOXTYPE_PROVIDER", "directml")
    monkeypatch.setenv("VOXTYPE_NUM_THREADS", "8")

    config = load_config(["--host", "127.0.0.1", "--port", "6016"])

    assert config.provider == "directml"
    assert config.num_threads == 8


def test_load_config_invalid_provider_falls_back_to_cpu(monkeypatch) -> None:
    monkeypatch.delenv("VOXTYPE_MODEL_DIR", raising=False)
    monkeypatch.setenv("VOXTYPE_PROVIDER", "not-a-provider")

    config = load_config(["--host", "127.0.0.1", "--port", "6016"])

    assert config.provider == "cpu"
