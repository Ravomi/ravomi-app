"""Real Wry child WebView -> Bevy input bridge, under a disposable Xvfb display.

Build app's canvas editor bundle and the root binary with --features react-editor.
Run: xvfb-run -a -s '-screen 0 1600x1000x24' env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET GDK_BACKEND=x11 \
  python3 app/tests/canvas_native_smoke.py target/release/project-graph-bevy-canvas
Synthetic composition events verify routing; they do not certify a real IME.
"""
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
    fixture = str(Path("fixtures/tutorial-shortcut-keys.mio").resolve(strict=True))
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    endpoint = f"http://127.0.0.1:{port}"

    def request(method, route, payload=None):
        data = None if payload is None else json.dumps(payload).encode()
        with urlopen(Request(endpoint + route, data=data, method=method,
                             headers={"Content-Type": "application/json"}), timeout=60) as response:
            value = json.load(response)["value"]
        if isinstance(value, dict) and "error" in value:
            raise RuntimeError(value)
        return value

    environment = {**os.environ, "TAURI_WEBVIEW_AUTOMATION": "true", "GDK_BACKEND": "x11"}
    environment.pop("WAYLAND_DISPLAY", None)
    environment.pop("WAYLAND_SOCKET", None)
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
                "webkitgtk:browserOptions": {
                    "binary": binary,
                    "args": [fixture, "--synthetic", "1", "--edit-node", "0", "--react-editor", *sys.argv[2:]],
                }
            }}})["sessionId"]
            prefix = f"/session/{session}"

            def script(source):
                return request("POST", prefix + "/execute/sync", {"script": source, "args": []})

            def wait_for(source):
                deadline = time.monotonic() + 20
                while not script(source):
                    if time.monotonic() > deadline:
                        raise AssertionError(f"Timed out: {source}")
                    time.sleep(0.1)

            wait_for("return !!document.querySelector('textarea')")
            script("window.results=[];window.addEventListener('hm:edit-result', e=>window.results.push(e.detail));")
            script("const t=document.querySelector('textarea');t.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:''}));Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(t,'ni hao');t.dispatchEvent(new Event('input',{bubbles:true}));")
            time.sleep(0.5)
            script("document.querySelector('textarea').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',bubbles:true}));")
            assert script("return window.results.length") == 0
            script("const t=document.querySelector('textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(t,'你好');t.dispatchEvent(new Event('input',{bubbles:true}));t.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'你好'}));")
            wait_for("return document.querySelector('textarea').value === '你好'")
            output = Path(os.environ.get("HM_NATIVE_SCREENSHOT", "results/react-editor-native.png"))
            output.parent.mkdir(parents=True, exist_ok=True)
            time.sleep(0.5)
            subprocess.run(["import", "-window", "root", str(output)], check=True, timeout=15)
            script("document.querySelector('textarea').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',code:'Enter',bubbles:true}));")
            wait_for("return window.results.length === 1")
            assert script("return window.results[0].accepted") is True
            print("PASS: same-window React/WebKit preedit -> Bevy draft -> acknowledged commit")
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
