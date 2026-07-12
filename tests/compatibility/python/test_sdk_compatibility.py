import json
import os
import socket
import subprocess
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.request import urlopen

from anthropic import Anthropic
from openai import OpenAI


class CaptureHandler(BaseHTTPRequestHandler):
    captures = []

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        self.__class__.captures.append((self.path, body, dict(self.headers)))
        if self.path == "/v1/messages":
            payload = {"id":"msg_mock","type":"message","role":"assistant","model":"mock","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}
        else:
            payload = {"id":"chatcmpl_mock","object":"chat.completion","created":1,"model":"mock","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}
        encoded = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_):
        pass


def free_port():
    with socket.socket() as candidate:
        candidate.bind(("127.0.0.1", 0))
        return candidate.getsockname()[1]


class GatewaySdkCompatibility(unittest.TestCase):
    def setUp(self):
        self.root = Path(__file__).resolve().parents[3]
        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), CaptureHandler)
        self.upstream_thread = threading.Thread(target=self.upstream.serve_forever, daemon=True)
        self.upstream_thread.start()
        self.gateway_port = free_port()
        self.gateway = None

    def tearDown(self):
        if self.gateway is not None:
            self.gateway.terminate()
            self.gateway.wait(timeout=10)
        self.upstream.shutdown()
        self.upstream.server_close()

    def start_gateway(self, protocol):
        environment = os.environ | {"LPG_UPSTREAM_API_KEY":"synthetic-upstream-key","LPG_LOCAL_API_KEY":"synthetic-local-key","LPG_MAPPING_KEY_HEX":"11" * 32}
        if os.name != "nt":
            environment["PATH"] = os.pathsep.join([str(Path.home() / ".cargo" / "bin"), "/usr/bin", "/bin", environment["PATH"]])
        self.gateway = subprocess.Popen(["cargo","run","--quiet","-p","lpg-cli","--","serve","--protocol",protocol,"--bind",f"127.0.0.1:{self.gateway_port}","--upstream-base-url",f"http://127.0.0.1:{self.upstream.server_port}"], cwd=self.root, env=environment)
        for _ in range(100):
            try:
                urlopen(f"http://127.0.0.1:{self.gateway_port}/healthz", timeout=0.1)
                return
            except Exception:
                time.sleep(0.05)
        self.fail("gateway did not start")

    def test_openai_sdk_uses_local_base_url_without_leaking_email(self):
        self.start_gateway("openai")
        response = OpenAI(base_url=f"http://127.0.0.1:{self.gateway_port}/v1", api_key="synthetic-local-key").chat.completions.create(model="mock", messages=[{"role":"user","content":"email sdk-canary@example.test"}])
        self.assertEqual(response.choices[0].message.content, "ok")
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"sdk-canary@example.test", body)

    def test_anthropic_sdk_uses_native_messages_without_leaking_email(self):
        self.start_gateway("anthropic")
        response = Anthropic(base_url=f"http://127.0.0.1:{self.gateway_port}", api_key="synthetic-local-key").messages.create(model="mock", max_tokens=8, messages=[{"role":"user","content":"email sdk-canary@example.test"}])
        self.assertEqual(response.content[0].text, "ok")
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"sdk-canary@example.test", body)


if __name__ == "__main__":
    unittest.main()
