from __future__ import annotations

import time
from dataclasses import dataclass, field


@dataclass
class VoiceSession:
    session_id: str
    language: str = "zh-CN"
    streaming: bool = False
    sample_rate: int = 16_000
    pcm_chunks: list[bytes] = field(default_factory=list)
    last_partial_text: str = ""
    last_partial_at: float = 0.0

    @property
    def pcm(self) -> bytes:
        return b"".join(self.pcm_chunks)

    def append_pcm(self, chunk: bytes) -> None:
        if chunk:
            self.pcm_chunks.append(bytes(chunk))

    def should_emit_partial(
        self,
        *,
        min_interval_s: float = 0.5,
        min_pcm_bytes: int = 32_000,
    ) -> bool:
        if not self.streaming:
            return False
        if len(self.pcm) < min_pcm_bytes:
            return False
        return (time.monotonic() - self.last_partial_at) >= min_interval_s
