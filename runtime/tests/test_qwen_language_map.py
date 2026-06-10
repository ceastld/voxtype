from __future__ import annotations

from voxtype_runtime.recognizer.onnx_gguf import _map_qwen_language


def test_zh_cn_maps_to_chinese() -> None:
    assert _map_qwen_language("zh-CN") == "Chinese"
    assert _map_qwen_language("Zh-cn") == "Chinese"


def test_auto_returns_none() -> None:
    assert _map_qwen_language("auto") is None
    assert _map_qwen_language("") is None


def test_unknown_locale_falls_back_to_auto() -> None:
    assert _map_qwen_language("xx-YY") is None
