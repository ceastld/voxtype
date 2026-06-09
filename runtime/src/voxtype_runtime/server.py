from __future__ import annotations

import asyncio
import logging

from typing import Any



from aiohttp import web



from voxtype_runtime.config import RuntimeConfig

from voxtype_runtime.protocol import PROTOCOL_VERSION, WS_SUBPROTOCOL, dumps

from voxtype_runtime.recognizer.base import Recognizer

from voxtype_runtime.router import VoiceSessionRouter



logger = logging.getLogger(__name__)





class VoiceRuntimeApp:

    def __init__(self, config: RuntimeConfig, recognizer: Recognizer) -> None:

        self._config = config

        self._recognizer = recognizer



    def health_payload(self) -> dict[str, Any]:

        return {

            "ok": True,

            "protocolVersion": PROTOCOL_VERSION,

            "runtimeVersion": self._config.runtime_version,

            "modelId": self._recognizer.model_id,

            "modelLoaded": self._recognizer.model_id != "stub",

            "ready": self._recognizer.model_id != "stub" and self._recognizer.ready,

            "executionProvider": getattr(
                self._recognizer, "execution_provider", "cpu"
            ),

        }



    async def health_handler(self, request: web.Request) -> web.Response:

        response = web.json_response(self.health_payload())

        response.headers["Access-Control-Allow-Origin"] = "*"

        return response



    async def options_handler(self, request: web.Request) -> web.Response:

        response = web.Response(status=204)

        response.headers["Access-Control-Allow-Origin"] = "*"

        response.headers["Access-Control-Allow-Methods"] = "GET, OPTIONS"

        response.headers["Access-Control-Allow-Headers"] = "Content-Type"

        return response



    async def websocket_handler(self, request: web.Request) -> web.WebSocketResponse:

        # One session router per WebSocket so launcher and main chat do not block each other.
        router = VoiceSessionRouter(self._recognizer)

        ws = web.WebSocketResponse(protocols=(WS_SUBPROTOCOL,))

        await ws.prepare(request)



        if ws.ws_protocol != WS_SUBPROTOCOL:

            logger.warning(

                "Client connected without %s subprotocol (got %r)",

                WS_SUBPROTOCOL,

                ws.ws_protocol,

            )

            await ws.close()

            return ws



        async for msg in ws:

            if msg.type == web.WSMsgType.BINARY:
                outbound = await asyncio.to_thread(router.handle_pcm, msg.data)
                for message in outbound:
                    await ws.send_str(dumps(message))
                continue



            if msg.type != web.WSMsgType.TEXT:

                if msg.type in (web.WSMsgType.CLOSE, web.WSMsgType.ERROR):

                    break

                continue



            payload = router.parse_control(msg.data)

            if payload is None:

                continue



            for outbound in router.handle_control(payload):

                await ws.send_str(dumps(outbound))



        if router.active_session is not None:
            router.clear_active_session()
        return ws



    def create_web_app(self) -> web.Application:

        app = web.Application()

        app.router.add_route("OPTIONS", "/health", self.options_handler)

        app.router.add_get("/health", self.health_handler)

        app.router.add_get("/", self.websocket_handler)

        return app





async def run_server(config: RuntimeConfig, recognizer: Recognizer) -> None:

    runtime = VoiceRuntimeApp(config, recognizer)

    app = runtime.create_web_app()

    runner = web.AppRunner(app)

    await runner.setup()

    site = web.TCPSite(runner, config.host, config.port)

    await site.start()

    logger.info(

        "voxtype-runtime %s listening on http://%s:%s/health (ws on /)",

        config.runtime_version,

        config.host,

        config.port,

    )

    try:

        import asyncio



        await asyncio.Event().wait()

    finally:

        await runner.cleanup()


