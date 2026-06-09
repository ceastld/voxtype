from __future__ import annotations

import json
import logging
import sys
from typing import BinaryIO

from voxtype_runtime.config import RuntimeConfig
from voxtype_runtime.protocol import dumps
from voxtype_runtime.recognizer.base import Recognizer
from voxtype_runtime.router import VoiceSessionRouter

logger = logging.getLogger(__name__)

FRAME_JSON = 0
FRAME_PCM = 1


def _read_exact(stream: BinaryIO, size: int) -> bytes | None:
    chunks: list[bytes] = []
    remaining = size
    while remaining > 0:
        chunk = stream.read(remaining)
        if not chunk:
            return None
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def write_json_frame(stream: BinaryIO, message: dict) -> None:
    payload = dumps(message).encode("utf-8")
    header = bytes([FRAME_JSON]) + len(payload).to_bytes(4, "big")
    stream.write(header + payload)
    stream.flush()


def write_pcm_frame(stream: BinaryIO, chunk: bytes) -> None:
    header = bytes([FRAME_PCM]) + len(chunk).to_bytes(4, "big")
    stream.write(header + chunk)
    stream.flush()


def run_stdio(config: RuntimeConfig, recognizer: Recognizer) -> None:
    router = VoiceSessionRouter(recognizer)
    stdin = sys.stdin.buffer
    stdout = sys.stdout.buffer

    write_json_frame(stdout, router.runtime_ready_payload(config.runtime_version))
    logger.info(
        "voxtype-runtime %s ready on stdio (model=%s)",
        config.runtime_version,
        recognizer.model_id,
    )

    while True:
        header = _read_exact(stdin, 5)
        if header is None:
            break

        kind = header[0]
        length = int.from_bytes(header[1:5], "big")
        payload = _read_exact(stdin, length)
        if payload is None:
            break

        outbound: list[dict] = []
        if kind == FRAME_JSON:
            parsed = router.parse_control(payload.decode("utf-8"))
            if parsed is not None:
                outbound = router.handle_control(parsed)
        elif kind == FRAME_PCM:
            outbound = router.handle_pcm(payload)

        for message in outbound:
            write_json_frame(stdout, message)
