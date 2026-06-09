from __future__ import annotations

import json
from typing import Any


PROTOCOL_VERSION = 1
WS_SUBPROTOCOL = "voxtype-voice-v1"


def dumps(message: dict[str, Any]) -> str:
    return json.dumps(message, ensure_ascii=False, separators=(",", ":"))


def loads(raw: str | bytes) -> dict[str, Any] | None:
    try:
        parsed = json.loads(raw)
    except (json.JSONDecodeError, TypeError):
        return None
    return parsed if isinstance(parsed, dict) else None
