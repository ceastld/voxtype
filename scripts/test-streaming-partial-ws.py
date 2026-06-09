"""Smoke-test streaming partials over voxtype-voice-v1 WebSocket."""

from __future__ import annotations

import argparse
import asyncio
import json
import uuid

import aiohttp


async def run(host: str, port: int, chunks: int) -> None:
    url = f"ws://{host}:{port}/"
    session_id = str(uuid.uuid4())
    chunk = b"\x00\x01" * 4_000  # 0.25s @ 16kHz mono s16le

    async with aiohttp.ClientSession() as session:
        async with session.ws_connect(url, protocols=["voxtype-voice-v1"]) as ws:
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
                )
            )
            started = await ws.receive(timeout=5)
            print("started:", started.data)

            partials: list[str] = []
            for i in range(chunks):
                await ws.send_bytes(chunk)
                try:
                    msg = await ws.receive(timeout=5)
                except asyncio.TimeoutError:
                    print(f"chunk {i + 1}: no message within 5s")
                    continue
                if msg.type != aiohttp.WSMsgType.TEXT:
                    print(f"chunk {i + 1}: non-text {msg.type}")
                    continue
                payload = json.loads(msg.data)
                print(f"chunk {i + 1}:", payload)
                if payload.get("type") == "partial":
                    partials.append(str(payload.get("text") or ""))

            await ws.send_str(
                json.dumps({"type": "session.end", "sessionId": session_id})
            )
            while True:
                msg = await ws.receive(timeout=5)
                if msg.type in (aiohttp.WSMsgType.CLOSE, aiohttp.WSMsgType.CLOSING):
                    break
                if msg.type == aiohttp.WSMsgType.TEXT:
                    print("end:", msg.data)
                    payload = json.loads(msg.data)
                    if payload.get("type") == "session.ended":
                        break

    print(f"partials received: {len(partials)}")
    if not partials:
        raise SystemExit("FAIL: no partial transcripts")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=6016)
    parser.add_argument("--chunks", type=int, default=4)
    args = parser.parse_args()
    asyncio.run(run(args.host, args.port, args.chunks))


if __name__ == "__main__":
    main()
