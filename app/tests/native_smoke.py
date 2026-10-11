"""Linux-only real WebKit/Tauri IPC smoke test; no user documents are used.

Run from the repository root after building the embedded frontend:
  xvfb-run -a python3 app/tests/native_smoke.py target/debug/hm-app
Requires WebKitWebDriver in PATH. This does not measure performance or IME.
"""

import base64
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import URLError
from urllib.request import Request, urlopen


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    endpoint = f"http://127.0.0.1:{port}"

    def request(method, route, payload=None):
        data = None if payload is None else json.dumps(payload).encode()
        with urlopen(Request(endpoint + route, data=data, method=method,
                             headers={"Content-Type": "application/json"}), timeout=30) as response:
            value = json.load(response)["value"]
        if isinstance(value, dict) and "error" in value:
            raise RuntimeError(value)
        return value

    environment = {**os.environ, "TAURI_WEBVIEW_AUTOMATION": "true", "GDK_BACKEND": "x11"}
    with tempfile.TemporaryFile(mode="w+") as log:
        driver = subprocess.Popen(["WebKitWebDriver", f"--port={port}"],
                                  env=environment, stdout=log, stderr=log)
        session = None
        try:
            deadline = time.monotonic() + 10
            while True:
                try:
                    request("GET", "/status")
                    break
                except URLError:
                    if time.monotonic() > deadline or driver.poll() is not None:
                        raise
                    time.sleep(0.1)
            session = request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "webkitgtk:browserOptions": {"binary": binary}
            }}})["sessionId"]
            prefix = f"/session/{session}"
            request("POST", prefix + "/window/rect", {"width": 1200, "height": 800})

            def script(source, args=None):
                return request("POST", prefix + "/execute/sync", {"script": source, "args": args or []})

            def wait_for(source):
                deadline = time.monotonic() + 15
                while not script(source):
                    if time.monotonic() > deadline:
                        raise AssertionError(f"Timed out: {source}")
                    time.sleep(0.1)

            def invoke(name, args=None):
                result = request("POST", prefix + "/execute/async", {
                    "script": "const done = arguments[2]; window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]).then(value => done({value}), error => done({error: String(error)}));",
                    "args": [name, args or {}],
                })
                if "error" in result:
                    raise RuntimeError(result["error"])
                return result["value"]

            wait_for("return !!document.querySelector('.workspace-heading button') && !document.querySelector('.workspace-heading button').disabled")
            script("document.querySelector('.workspace-heading button').click()")
            wait_for("return !!document.querySelector('.object-card') && !!document.querySelector('textarea')")
            script("const field = document.querySelector('textarea'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(field, '真实原生 IPC 中文'); field.dispatchEvent(new Event('input', {bubbles: true}));")
            wait_for("return !document.querySelector('.inspector button').disabled")
            script("document.querySelector('.inspector button').click()")
            wait_for("return document.querySelector('.object-text').textContent === '真实原生 IPC 中文'")
            snapshot = invoke("session_snapshot")
            assert snapshot["dirty"] and snapshot["nodes"][0]["text"] == "真实原生 IPC 中文"
            if len(sys.argv) > 2:
                screenshot = Path(sys.argv[2])
                screenshot.parent.mkdir(parents=True, exist_ok=True)
                screenshot.write_bytes(base64.b64decode(request("GET", prefix + "/screenshot")))
            with tempfile.TemporaryDirectory(prefix="hm-native-smoke-") as directory:
                path = str(Path(directory) / "中文.prg")
                saved = invoke("session_execute", {"command": {"type": "save", "path": path}})
                assert not saved["dirty"] and Path(path).is_file()
                invoke("session_execute", {"command": {"type": "new", "discard": False}})
                reopened = invoke("session_execute", {"command": {"type": "open", "path": path, "discard": False}})
                assert reopened["nodes"][0]["text"] == "真实原生 IPC 中文"
            assert not script("return !!document.querySelector('[role=alert]')")
            print("PASS: native React interaction → Tauri IPC → Rust edit/save/reopen (temporary PRG)")
        finally:
            try:
                if session:
                    request("DELETE", f"/session/{session}")
            finally:
                driver.terminate()
                try:
                    driver.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    driver.kill()
                    driver.wait()
                log.seek(0)
                diagnostics = log.read()
                if diagnostics:
                    print(diagnostics, file=sys.stderr)


if __name__ == "__main__":
    main()
