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


def _infer_preset(config: RuntimeConfig) -> str:
    if config.model_type:
        return resolve_preset(config.model_type)
    if config.model_dir is not None:
        name = config.model_dir.name.lower()
        if "paraformer" in name:
            return "paraformer"
    return "sensevoice"


def _plugin_root_for_dir(model_dir: Path) -> Path:
    if model_dir.parent.name == "models":
        return model_dir.parent.parent
    return plugin_data_root()


def resolve_runtime_model(config: RuntimeConfig) -> RuntimeConfig:
    """Validate configured model; repair or fail before recognizer load."""
    preset = _infer_preset(config)
    plugin_root = plugin_data_root()
    dest = config.model_dir or target_dir(plugin_root, preset)
    ready, err = describe_model_status(dest, preset=preset)

    if ready:
        return RuntimeConfig(
            host=config.host,
            port=config.port,
            transport=config.transport,
            model_dir=dest.resolve(),
            model_type=config.model_type or preset,
            provider=config.provider,
            num_threads=config.num_threads,
            log_level=config.log_level,
        )

    auto = _auto_download_enabled()
    has_partial = dest.exists()

    if has_partial:
        logger.warning("ASR model invalid at %s: %s", dest, err)
        if auto:
            remove_model_dir(dest)
        else:
            message = err or "model incomplete"
            message = f"{message}; re-download the model in VoxType settings"
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
        return RuntimeConfig(
            host=config.host,
            port=config.port,
            transport=config.transport,
            model_dir=repaired.resolve(),
            model_type=config.model_type or preset,
            provider=config.provider,
            num_threads=config.num_threads,
            log_level=config.log_level,
        )

    message = err or "model not installed or incomplete"
    logger.error("ASR model unavailable: %s", message)
    raise SystemExit(1)
