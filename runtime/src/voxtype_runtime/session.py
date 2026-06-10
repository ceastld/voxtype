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
    last_partial_pcm_len: int = 0
    partial_min_interval_s: float = 0.15
    partial_min_pcm_bytes: int = 6_400
    partial_min_new_pcm_bytes: int = 3_200

    @property
    def pcm(self) -> bytes:
        return b"".join(self.pcm_chunks)

    def append_pcm(self, chunk: bytes) -> None:
        if chunk:
            self.pcm_chunks.append(bytes(chunk))

    def should_emit_partial(self) -> bool:
        if not self.streaming:
            return False
        pcm_len = len(self.pcm)
        if pcm_len < self.partial_min_pcm_bytes:
            return False
        if pcm_len - self.last_partial_pcm_len < self.partial_min_new_pcm_bytes:
            return False
        return (time.monotonic() - self.last_partial_at) >= self.partial_min_interval_s

    def mark_partial_emitted(self, text: str) -> None:
        self.last_partial_text = text
        self.last_partial_at = time.monotonic()
        self.last_partial_pcm_len = len(self.pcm)
