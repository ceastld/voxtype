from __future__ import annotations

from pathlib import Path

import pytest

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


def test_invalid_partial_not_deleted_when_auto_download_disabled(
    tmp_path: Path,
    monkeypatch,
) -> None:
    dest = tmp_path / "models" / "sensevoice"
    dest.mkdir(parents=True)
    (dest / "partial.bin").write_bytes(b"x")

    monkeypatch.setenv("VOXTYPE_AUTO_DOWNLOAD_MODEL", "0")
    monkeypatch.setenv("VOXTYPE_PLUGIN_ROOT", str(tmp_path))

    with pytest.raises(SystemExit):
        resolve_runtime_model(_base_config(dest))

    assert dest.is_dir()
    assert (dest / "partial.bin").is_file()
