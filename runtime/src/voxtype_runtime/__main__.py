from __future__ import annotations

import asyncio
import logging

from voxtype_runtime.config import configure_logging, load_config
from voxtype_runtime.model_bootstrap import resolve_runtime_model
from voxtype_runtime.recognizer import create_recognizer
from voxtype_runtime.server import run_server
from voxtype_runtime.stdio_transport import run_stdio


def main(argv: list[str] | None = None) -> None:
    import sys

    if argv is None:
        argv = sys.argv[1:]

    if argv and argv[0] == "download-model":
        from voxtype_runtime.download_model import main as download_main

        download_main(argv[1:])
        return

    if argv and argv[0] == "check-model":
        from voxtype_runtime.download_model import check_main

        check_main(argv[1:])
        return

    base_config = load_config(argv)
    try:
        config = resolve_runtime_model(base_config)
    except SystemExit:
        raise
    except Exception as exc:
        logging.getLogger(__name__).error("ASR model bootstrap failed: %s", exc)
        raise SystemExit(1) from exc

    configure_logging(config.log_level)
    logging.getLogger(__name__).info(
        "Starting voxtype-runtime (model_dir=%s)",
        config.model_dir,
    )
    recognizer = create_recognizer(
        config.model_dir,
        config.model_type,
        provider=config.provider,
        num_threads=config.num_threads,
    )
    if config.model_dir is not None and recognizer.model_id == "stub":
        logging.getLogger(__name__).error(
            "Failed to load sherpa-onnx model from %s",
            config.model_dir,
        )
        raise SystemExit(1)
    try:
        if config.transport == "stdio":
            run_stdio(config, recognizer)
        else:
            asyncio.run(run_server(config, recognizer))
    except KeyboardInterrupt:
        logging.getLogger(__name__).info("Shutting down")

if __name__ == "__main__":
    main()
