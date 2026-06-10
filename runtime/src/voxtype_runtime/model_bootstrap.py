"""Validate and optionally repair ASR model files before runtime startup."""

from __future__ import annotations

import logging
import os
from pathlib import Path

from voxtype_runtime.config import RuntimeConfig
from voxtype_runtime.download_model import (
    describe_model_status,
    ensure_asr_model,
    remove_model_dir,
    resolve_preset,
    target_dir,
)
from voxtype_runtime.paths import plugin_data_root

logger = logging.getLogger(__name__)


def _auto_download_enabled() -> bool:
    return os.environ.get("VOXTYPE_AUTO_DOWNLOAD_MODEL", "1") != "0"


def _stub_allowed() -> bool:
    return os.environ.get("VOXTYPE_ALLOW_STUB", "0") == "1"


def _infer_preset_from_dir(model_dir: Path) -> str | None:
    name = model_dir.name.lower()
    if "qwen" in name:
        return "qwen_asr"
    if "fun" in name and "nano" in name:
        return "fun_asr_nano"
    if "paraformer" in name:
        return "paraformer"
    if "sensevoice" in name or "sense-voice" in name:
        return "sensevoice"
    if "whisper" in name:
        return "whisper"
    return None


def _infer_preset(config: RuntimeConfig) -> str:
    if config.model_type:
        return resolve_preset(config.model_type)
    if config.model_dir is not None:
        from_dir = _infer_preset_from_dir(config.model_dir)
        if from_dir is not None:
            return from_dir
    return "sensevoice"


def _hybrid_model_type(model_dir: Path) -> str | None:
    try:
        from voxtype_runtime.recognizer.onnx_gguf_layout import (
            has_funasr_hybrid_layout,
            has_qwen_hybrid_layout,
        )
    except Exception:
        return None
    if has_qwen_hybrid_layout(model_dir):
        return "qwen_asr"
    if has_funasr_hybrid_layout(model_dir):
        return "fun_asr_nano"
    return None


def _plugin_root_for_dir(model_dir: Path) -> Path:
    if model_dir.parent.name == "models":
        return model_dir.parent.parent
    return plugin_data_root()


def _config_with_model_dir(
    config: RuntimeConfig,
    dest: Path,
    model_type: str,
) -> RuntimeConfig:
    return RuntimeConfig(
        host=config.host,
        port=config.port,
        transport=config.transport,
        model_dir=dest.resolve(),
        model_type=model_type,
        provider=config.provider,
        num_threads=config.num_threads,
        log_level=config.log_level,
    )


def resolve_runtime_model(config: RuntimeConfig) -> RuntimeConfig:
    """Validate configured model; repair or fail before recognizer load."""
    preset = _infer_preset(config)
    plugin_root = plugin_data_root()
    dest = config.model_dir or target_dir(plugin_root, preset)

    hybrid_type = _hybrid_model_type(dest) if dest.is_dir() else None
    if hybrid_type is not None:
        logger.info("ASR hybrid model ready at %s (type=%s)", dest, hybrid_type)
        return _config_with_model_dir(
            config,
            dest,
            config.model_type or hybrid_type,
        )

    ready, err = describe_model_status(dest, preset=preset)

    if ready:
        return _config_with_model_dir(config, dest, config.model_type or preset)

    auto = _auto_download_enabled()
    has_partial = dest.exists()

    if has_partial:
        logger.warning("ASR model invalid at %s: %s", dest, err)
        if auto:
            remove_model_dir(dest)
        elif not _stub_allowed():
            message = (
                f"{err or 'model incomplete'}; "
                "re-download the model in VoxType settings"
            )
            logger.error("ASR model unavailable: %s", message)
            raise SystemExit(1)

    if auto:
        logger.info("Repairing ASR model (preset=%s)...", preset)
        root = _plugin_root_for_dir(dest) if config.model_dir else plugin_root
        repaired = ensure_asr_model(root, preset=preset, force=False)
        ready, err = describe_model_status(repaired, preset=preset)
        if not ready:
            logger.error("ASR model repair failed: %s", err)
            raise SystemExit(1)
        return _config_with_model_dir(config, repaired, config.model_type or preset)

    message = err or "model not installed or incomplete"
    if has_partial:
        message = f"{message}; re-download the model in VoxType settings"
    if _stub_allowed():
        logger.warning("ASR model unavailable; starting stub recognizer: %s", message)
        return RuntimeConfig(
            host=config.host,
            port=config.port,
            transport=config.transport,
            model_dir=None,
            model_type=config.model_type or preset,
            provider=config.provider,
            num_threads=config.num_threads,
            log_level=config.log_level,
        )
    logger.error("ASR model unavailable: %s", message)
    raise SystemExit(1)
