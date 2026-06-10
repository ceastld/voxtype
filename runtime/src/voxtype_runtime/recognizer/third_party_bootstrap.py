"""Expose HaujetZhao ONNX+GGUF third-party packages to import."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from voxtype_runtime.paths import is_frozen


def voxtype_repo_root() -> Path:
    if is_frozen():
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[4]


def third_party_roots() -> list[Path]:
    env = os.environ.get("VOXTYPE_THIRD_PARTY_ROOT", "").strip()
    roots: list[Path] = []
    if env:
        roots.append(Path(env).expanduser().resolve())
    if is_frozen():
        bundle_root = Path(getattr(sys, "_MEIPASS", Path(sys.executable).resolve().parent))
        roots.extend(
            [
                bundle_root / "third_party" / "Fun-ASR-GGUF",
                bundle_root / "third_party" / "Qwen3-ASR-GGUF",
            ]
        )
    repo = voxtype_repo_root()
    roots.extend(
        [
            repo / "third_party" / "Fun-ASR-GGUF",
            repo / "third_party" / "Qwen3-ASR-GGUF",
        ]
    )
    seen: set[str] = set()
    ordered: list[Path] = []
    for root in roots:
        key = str(root)
        if key not in seen and root.is_dir():
            seen.add(key)
            ordered.append(root)
    return ordered


def ensure_third_party_paths() -> list[Path]:
    added: list[Path] = []
    for root in third_party_roots():
        text = str(root)
        if text not in sys.path:
            sys.path.insert(0, text)
            added.append(root)
    return added
