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
from openai import BadRequestError, OpenAI


class CaptureHandler(BaseHTTPRequestHandler):
    captures = []

    def do_POST(self):
        body = self.rfile.read(int(self.headers["Content-Length"]))
        self.__class__.captures.append((self.path, body, dict(self.headers)))
        request = json.loads(body)
        if request.get("model") == "force-error":
            payload = {"error": {"message": "forced mock error", "type": "invalid_request_error"}}
            encoded = json.dumps(payload).encode()
            self.send_response(400)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)
            return
        if request.get("stream") and self.path == "/chat/completions":
            payload = {
                "id": "chatcmpl_stream_mock",
                "object": "chat.completion.chunk",
                "created": 1,
                "model": "mock",
                "choices": [{"index": 0, "delta": {"content": "ok"}, "finish_reason": "stop"}],
            }
            encoded = f"data: {json.dumps(payload)}\n\ndata: [DONE]\n\n".encode()
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.send_header("content-length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)
            return
        if self.path == "/v1/messages/count_tokens":
            payload = {"input_tokens": 1}
        elif self.path == "/v1/messages":
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
    @classmethod
    def setUpClass(cls):
        cls.root = Path(__file__).resolve().parents[3]
        cls.command_environment = os.environ.copy()
        if os.name != "nt":
            cls.command_environment["PATH"] = os.pathsep.join(
                [str(Path.home() / ".cargo" / "bin"), "/usr/bin", "/bin", cls.command_environment["PATH"]]
            )
        build = subprocess.run(
            ["cargo", "build", "--quiet", "--locked", "-p", "lpg-cli"],
            cwd=cls.root,
            env=cls.command_environment,
            capture_output=True,
            text=True,
            timeout=120,
        )
        if build.returncode != 0:
            raise RuntimeError(f"gateway build failed: {build.stderr.strip()}")
        target_dir = Path(cls.command_environment.get("CARGO_TARGET_DIR", cls.root / "target"))
        if not target_dir.is_absolute():
            target_dir = cls.root / target_dir
        cls.gateway_executable = target_dir / "debug" / ("lpg-cli.exe" if os.name == "nt" else "lpg-cli")

    def setUp(self):
        CaptureHandler.captures = []
        self.upstream = ThreadingHTTPServer(("127.0.0.1", 0), CaptureHandler)
        self.upstream_thread = threading.Thread(target=self.upstream.serve_forever, daemon=True)
        self.upstream_thread.start()
        self.gateway_port = free_port()
        self.gateway = None

    def tearDown(self):
        if self.gateway is not None:
            if self.gateway.poll() is None:
                self.gateway.terminate()
            self.gateway.wait(timeout=10)
            if self.gateway.stderr is not None:
                self.gateway.stderr.close()
        self.upstream.shutdown()
        self.upstream.server_close()

    def start_gateway(self, protocol):
        environment = self.command_environment | {"LPG_UPSTREAM_API_KEY":"synthetic-upstream-key","LPG_LOCAL_API_KEY":"synthetic-local-key","LPG_MAPPING_KEY_HEX":"11" * 32}
        self.gateway = subprocess.Popen(
            [self.gateway_executable, "serve", "--protocol", protocol, "--bind", f"127.0.0.1:{self.gateway_port}", "--upstream-base-url", f"http://127.0.0.1:{self.upstream.server_port}"],
            cwd=self.root,
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        for _ in range(300):
            try:
                urlopen(f"http://127.0.0.1:{self.gateway_port}/healthz", timeout=0.1)
                return
            except Exception:
                if self.gateway.poll() is not None:
                    break
                time.sleep(0.05)
        stderr = self.gateway.stderr.read().strip() if self.gateway.stderr else ""
        if self.gateway.stderr is not None:
            self.gateway.stderr.close()
        self.fail(f"gateway did not start{': ' + stderr if stderr else ''}")

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

    def test_openai_sdk_stream_tools_and_errors_use_the_gateway(self):
        self.start_gateway("openai")
        client = OpenAI(base_url=f"http://127.0.0.1:{self.gateway_port}/v1", api_key="synthetic-local-key")
        stream = client.chat.completions.create(model="mock", stream=True, messages=[{"role":"user","content":"stream-canary@example.test"}])
        self.assertEqual("".join(chunk.choices[0].delta.content or "" for chunk in stream), "ok")
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"stream-canary@example.test", body)
        client.chat.completions.create(model="mock", messages=[{"role":"user","content":"tool-canary@example.test"}], tools=[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}])
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"tool-canary@example.test", body)
        with self.assertRaises(BadRequestError):
            client.chat.completions.create(model="force-error", messages=[{"role":"user","content":"error-canary@example.test"}])
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"error-canary@example.test", body)

    def test_anthropic_sdk_counts_tokens_without_leaking_email(self):
        self.start_gateway("anthropic")
        response = Anthropic(base_url=f"http://127.0.0.1:{self.gateway_port}", api_key="synthetic-local-key").messages.count_tokens(model="mock", messages=[{"role":"user","content":"count-canary@example.test"}])
        self.assertEqual(response.input_tokens, 1)
        _, body, _ = CaptureHandler.captures[-1]
        self.assertNotIn(b"count-canary@example.test", body)


if __name__ == "__main__":
    unittest.main()
