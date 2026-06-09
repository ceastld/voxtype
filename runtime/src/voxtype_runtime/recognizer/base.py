from __future__ import annotations

from typing import Protocol


class Recognizer(Protocol):
    model_id: str
    ready: bool
    execution_provider: str

    def transcribe(self, pcm_s16le: bytes, *, sample_rate: int, language: str) -> str:
        """Return recognized text (may be empty)."""
