from __future__ import annotations

import logging
import time
from typing import Any

from voxtype_runtime.protocol import PROTOCOL_VERSION, loads
from voxtype_runtime.recognizer.base import Recognizer
from voxtype_runtime.session import VoiceSession

logger = logging.getLogger(__name__)


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
            return []
        if not text or text == session.last_partial_text:
            return []
        session.last_partial_text = text
        session.last_partial_at = time.monotonic()
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

        self._active = VoiceSession(
            session_id=session_id,
            language=str(payload.get("language") or "zh-CN"),
            streaming=bool(payload.get("streaming")),
            sample_rate=int(payload.get("sampleRate") or 16_000),
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
