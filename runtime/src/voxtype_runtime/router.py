from __future__ import annotations

import logging
from typing import Any

from voxtype_runtime.protocol import PROTOCOL_VERSION, loads
from voxtype_runtime.recognizer.base import Recognizer
from voxtype_runtime.session import VoiceSession

logger = logging.getLogger(__name__)

# ONNX+GGUF decode is heavy; throttle partials but keep live overlay updates.
_HYBRID_MODEL_IDS = frozenset({"qwen_asr", "fun_asr_nano"})


def _partial_policy(model_id: str) -> tuple[float, int, int]:
    """Return (min_interval_s, min_pcm_bytes, min_new_pcm_bytes)."""
    if model_id in _HYBRID_MODEL_IDS:
        # ~1s before first partial, then every ~0.8s if >=0.5s new audio
        return 0.8, 32_000, 16_000
    return 0.15, 6_400, 3_200


class VoiceSessionRouter:
    """Shared session state machine for WebSocket and stdio transports."""

    def __init__(self, recognizer: Recognizer) -> None:
        self._recognizer = recognizer
        self._active: VoiceSession | None = None

    @property
    def recognizer(self) -> Recognizer:
        return self._recognizer

    @property
    def active_session(self) -> VoiceSession | None:
        return self._active

    def runtime_ready_payload(self, runtime_version: str) -> dict[str, Any]:
        return {
            "type": "runtime.ready",
            "protocolVersion": PROTOCOL_VERSION,
            "runtimeVersion": runtime_version,
            "modelId": self._recognizer.model_id,
            "modelLoaded": self._recognizer.model_id != "stub",
            "ready": self._recognizer.model_id != "stub" and self._recognizer.ready,
        }

    def handle_control(self, payload: dict[str, Any]) -> list[dict[str, Any]]:
        msg_type = payload.get("type")

        if msg_type == "ping":
            return [
                {
                    "type": "pong",
                    "id": payload.get("id"),
                    "protocolVersion": PROTOCOL_VERSION,
                }
            ]

        if msg_type == "session.start":
            return self._handle_session_start(payload)

        if msg_type == "session.end":
            return self._handle_session_end(payload)

        if msg_type == "session.cancel":
            return self._handle_session_cancel(payload)

        return []

    def handle_pcm(self, chunk: bytes) -> list[dict[str, Any]]:
        if self._active is None or not chunk:
            return []
        self._active.append_pcm(chunk)
        if not self._active.streaming:
            return []
        return self._maybe_partial(self._active)

    def _maybe_partial(self, session: VoiceSession) -> list[dict[str, Any]]:
        if not session.should_emit_partial():
            return []
        try:
            text = self._recognizer.transcribe(
                session.pcm,
                sample_rate=session.sample_rate,
                language=session.language,
            )
        except Exception:
            logger.debug("partial transcribe failed", exc_info=True)
            return []
        if not text or text == session.last_partial_text:
            return []
        session.mark_partial_emitted(text)
        return [
            {
                "type": "partial",
                "sessionId": session.session_id,
                "text": text,
            }
        ]

    def _handle_session_start(self, payload: dict[str, Any]) -> list[dict[str, Any]]:
        if self._active is not None:
            return [
                {
                    "type": "error",
                    "sessionId": payload.get("sessionId"),
                    "code": "busy",
                    "message": "Session already active",
                }
            ]

        if not self._recognizer.ready:
            return [
                {
                    "type": "error",
                    "sessionId": payload.get("sessionId"),
                    "code": "not_ready",
                    "message": "Model is not ready",
                }
            ]

        session_id = str(payload.get("sessionId") or "")
        if not session_id:
            return [
                {
                    "type": "error",
                    "code": "invalid_session",
                    "message": "sessionId required",
                }
            ]

        interval, min_pcm, min_new = _partial_policy(self._recognizer.model_id)
        self._active = VoiceSession(
            session_id=session_id,
            language=str(payload.get("language") or "zh-CN"),
            streaming=bool(payload.get("streaming")),
            sample_rate=int(payload.get("sampleRate") or 16_000),
            partial_min_interval_s=interval,
            partial_min_pcm_bytes=min_pcm,
            partial_min_new_pcm_bytes=min_new,
        )
        return [{"type": "session.started", "sessionId": session_id}]

    def _handle_session_end(self, payload: dict[str, Any]) -> list[dict[str, Any]]:
        session_id = str(payload.get("sessionId") or "")
        if self._active is None or self._active.session_id != session_id:
            return [
                {
                    "type": "error",
                    "sessionId": session_id,
                    "code": "invalid_session",
                    "message": "No matching active session",
                }
            ]

        active = self._active
        try:
            text = self._recognizer.transcribe(
                active.pcm,
                sample_rate=active.sample_rate,
                language=active.language,
            )
            confidence = 0.9 if text else 0.0
        except Exception as exc:
            logger.exception("Recognition failed")
            self._active = None
            return [
                {
                    "type": "error",
                    "sessionId": session_id,
                    "code": "recognition_failed",
                    "message": str(exc),
                }
            ]

        self._active = None
        return [
            {
                "type": "final",
                "sessionId": session_id,
                "text": text,
                "confidence": confidence,
            },
            {"type": "session.ended", "sessionId": session_id},
        ]

    def _handle_session_cancel(self, payload: dict[str, Any]) -> list[dict[str, Any]]:
        session_id = str(payload.get("sessionId") or "")
        if self._active is not None and self._active.session_id == session_id:
            self._active = None
        return [{"type": "session.ended", "sessionId": session_id}]

    def clear_active_session(self) -> None:
        self._active = None

    @staticmethod
    def parse_control(raw: str | bytes) -> dict[str, Any] | None:
        return loads(raw)
