"""Resolve install / dev / PyInstaller paths."""

from __future__ import annotations

import os
import sys
from pathlib import Path


def is_frozen() -> bool:
    return bool(getattr(sys, "frozen", False))


def plugin_data_root() -> Path:
    """Writable data root (models/, logs/, settings.json)."""
    env_root = os.environ.get("VOXTYPE_DATA_ROOT", "").strip()
    if env_root:
        return Path(env_root).expanduser().resolve()
    local = os.environ.get("LOCALAPPDATA", "").strip()
    if local:
        return Path(local) / "VoxType"
    if is_frozen():
        exe = Path(sys.executable).resolve()
        if exe.parent.name == "runtime":
            return exe.parent.parent
        return exe.parent
    return Path(__file__).resolve().parents[2]


def default_models_dir() -> Path:
    return plugin_data_root() / "models"


def repo_models_dir() -> Path:
    """Directory containing sensevoice-model-identity.json (dev tree or PyInstaller bundle)."""
    if is_frozen():
        meipass = getattr(sys, "_MEIPASS", None)
        if meipass:
            return Path(meipass) / "models"
        return Path(sys.executable).resolve().parent / "models"
    return Path(__file__).resolve().parents[2] / "models"


def sensevoice_identity_path() -> Path:
    return repo_models_dir() / "sensevoice-model-identity.json"
