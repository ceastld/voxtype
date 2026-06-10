from __future__ import annotations

from voxtype_runtime.router import _partial_policy
from voxtype_runtime.session import VoiceSession


def test_hybrid_partial_policy_is_slower() -> None:
    interval, min_pcm, min_new = _partial_policy("qwen_asr")
    assert interval >= 0.8
    assert min_pcm >= 32_000
    assert min_new >= 16_000


def test_session_requires_new_audio_for_partial() -> None:
    session = VoiceSession(
        session_id="s1",
        streaming=True,
        partial_min_interval_s=0.0,
        partial_min_pcm_bytes=100,
        partial_min_new_pcm_bytes=50,
    )
    session.append_pcm(b"\x00" * 120)
    assert session.should_emit_partial()
    session.mark_partial_emitted("hello")
    session.append_pcm(b"\x00" * 10)
    assert not session.should_emit_partial()
    session.append_pcm(b"\x00" * 60)
    assert session.should_emit_partial()
