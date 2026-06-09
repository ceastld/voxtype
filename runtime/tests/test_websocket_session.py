from __future__ import annotations

import json
import uuid

import pytest
from aiohttp.test_utils import TestClient, TestServer

from voxtype_runtime.config import RuntimeConfig
from voxtype_runtime.recognizer.stub import StubRecognizer
from voxtype_runtime.server import VoiceRuntimeApp


class _ReadyStubRecognizer(StubRecognizer):
    ready = True


@pytest.fixture
async def voice_client() -> TestClient:
    config = RuntimeConfig(
        host="127.0.0.1",
        port=6016,
        transport="tcp",
        model_dir=None,
        model_type=None,
        provider="cpu",
        num_threads=4,
        log_level="WARNING",
    )
    app = VoiceRuntimeApp(config, _ReadyStubRecognizer()).create_web_app()
    server = TestServer(app)
    client = TestClient(server)
    await client.start_server()
    try:
        yield client
    finally:
        await client.close()


@pytest.mark.asyncio
async def test_session_start_returns_started(voice_client: TestClient) -> None:
    session_id = str(uuid.uuid4())
    async with voice_client.ws_connect("/", protocols=["voxtype-voice-v1"]) as ws:
        await ws.send_str(
            json.dumps(
                {
                    "type": "session.start",
                    "sessionId": session_id,
                    "language": "zh-CN",
                    "streaming": True,
                    "sampleRate": 16000,
                    "channels": 1,
                    "encoding": "pcm_s16le",
                },
                ensure_ascii=False,
            ),
        )
        msg = await ws.receive(timeout=3)
        assert msg.type.name == "TEXT"
        payload = json.loads(msg.data)
        assert payload == {"type": "session.started", "sessionId": session_id}


@pytest.mark.asyncio
async def test_streaming_pcm_emits_partial(voice_client: TestClient) -> None:
    session_id = str(uuid.uuid4())
    chunk = b"\x00\x01" * 4_000  # 0.25s @ 16kHz mono s16le

    async with voice_client.ws_connect("/", protocols=["voxtype-voice-v1"]) as ws:
        await ws.send_str(
            json.dumps(
                {
                    "type": "session.start",
                    "sessionId": session_id,
                    "language": "zh-CN",
                    "streaming": True,
                    "sampleRate": 16000,
                    "channels": 1,
                    "encoding": "pcm_s16le",
                },
                ensure_ascii=False,
            ),
        )
        started = await ws.receive(timeout=3)
        assert json.loads(started.data)["type"] == "session.started"

        await ws.send_bytes(chunk)
        msg = await ws.receive(timeout=3)
        assert msg.type.name == "TEXT"
        payload = json.loads(msg.data)
        assert payload["type"] == "partial"
        assert payload["sessionId"] == session_id
        assert str(payload.get("text") or "").startswith("[stub]")
