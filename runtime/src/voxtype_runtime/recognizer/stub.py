from __future__ import annotations

import struct
from voxtype_runtime.recognizer.base import Recognizer


class StubRecognizer:
    model_id = "stub"
    ready = False
    execution_provider = "cpu"

    def transcribe(self, pcm_s16le: bytes, *, sample_rate: int, language: str) -> str:
        if len(pcm_s16le) < sample_rate * 2 // 5:
            return ""
        seconds = len(pcm_s16le) / (sample_rate * 2)
        return f"[stub] 收到�?{seconds:.1f}s 音频（{language}�?
