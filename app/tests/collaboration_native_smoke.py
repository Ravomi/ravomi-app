"""Real Linux WebKit IPC, embedded Pi and explicit generated-code test.

Requires DEEPSEEK_API_KEY, WebKitWebDriver and a Tauri debug build with embedded UI.
Run: xvfb-run -a python3 app/tests/collaboration_native_smoke.py target/debug/hm-app
Uses temporary source/data directories; never reads user project files.
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
from urllib.request import Request, urlopen
from urllib.error import URLError


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="ravomi-ai-native-") as temporary:
        root = Path(temporary)
        project = root / "project"
        project.mkdir()
        (project / "math.cjs").write_text("module.exports = {};\n")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        endpoint = f"http://127.0.0.1:{port}"

        def request(method, route, payload=None):
            data = None if payload is None else json.dumps(payload).encode()
            with urlopen(Request(endpoint + route, data=data, method=method,
                                 headers={"Content-Type": "application/json"}), timeout=40) as response:
                result = json.load(response)["value"]
            if isinstance(result, dict) and "error" in result:
                raise RuntimeError(result["error"])
            return result

        env = {**os.environ, "TAURI_WEBVIEW_AUTOMATION": "true", "GDK_BACKEND": "x11", "XDG_DATA_HOME": str(root / "data")}
        with tempfile.TemporaryFile() as log:
            driver = subprocess.Popen(["WebKitWebDriver", f"--port={port}"], env=env, stdout=log, stderr=log)
            session = None
            try:
                deadline = time.monotonic() + 10
                while True:
                    try:
                        request("GET", "/status")
                        break
                    except URLError:
                        if time.monotonic() > deadline:
                            raise
                        time.sleep(0.1)
                session = request("POST", "/session", {"capabilities": {"alwaysMatch": {
                    "webkitgtk:browserOptions": {"binary": binary}
                }}})["sessionId"]
                prefix = f"/session/{session}"

                def invoke(name, args=None):
                    return request("POST", prefix + "/execute/async", {
                        "script": "const done=arguments[2]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(value=>done({value}),error=>done({error:String(error)}));",
                        "args": [name, args or {}],
                    })["value"]

                def command(value):
                    return invoke("ai_command", {"command": value})["workflows"][0]

                def wait(predicate):
                    deadline = time.monotonic() + 240
                    while time.monotonic() < deadline:
                        workflow = invoke("ai_snapshot")["workflows"][0]
                        failures = [(r["taskId"], r["error"]) for r in workflow["runs"] if r["status"] in ("failed", "storage_blocked")]
                        if failures:
                            detail = json.dumps(failures, ensure_ascii=False)
                            key = os.environ.get("DEEPSEEK_API_KEY")
                            raise AssertionError(detail.replace(key, "[redacted]") if key else detail)
                        if predicate(workflow):
                            return workflow
                        time.sleep(0.2)
                    raise AssertionError("Native AI workflow timed out")

                assert invoke("ai_scan", {"project": str(project)}) == ["math.cjs"]
                workflow = command({"type": "create", "project": str(project), "paths": ["math.cjs"], "goal": "实现 CommonJS add 与 multiply。两个并行分支分别新增 add.cjs 和 multiply.cjs，不修改 math.cjs。子 Agent 检查 add；最终汇总 Agent 修改 math.cjs，require 两个模块并导出函数，新增 math.test.cjs 用 node:test 验证，不引入依赖。", "model": "deepseek/deepseek-flash", "test_command": ["node", "--test"]})
                workflow = wait(lambda w: w["draftPlan"] is not None)
                identifier = workflow["id"]
                tasks = workflow["draftPlan"]["tasks"]
                def ancestors(identifier):
                    deps = next(t["dependencies"] for t in tasks if t["id"] == identifier)
                    return set(deps).union(*(ancestors(d) for d in deps))
                assert any(a["id"] != b["id"] and a["id"] not in ancestors(b["id"]) and b["id"] not in ancestors(a["id"]) for a in tasks for b in tasks)
                assert any(t["parentAgent"] for t in tasks)
                command({"type": "set_plan", "id": identifier, "plan": workflow["draftPlan"]})
                command({"type": "start", "id": identifier})
                # Real Rust document mutation while models work.
                assert invoke("session_snapshot")["nodes"] == []
                document = invoke("session_execute", {"command": {"type": "add_text", "x": 0, "y": 0}})
                document = invoke("session_execute", {"command": {"type": "edit_text", "id": document["nodes"][0]["id"], "text": "Agent 运行期间编辑独立模块"}})
                assert document["dirty"] and document["nodes"][0]["text"] == "Agent 运行期间编辑独立模块"
                workflow = wait(lambda w: len(w["runs"]) == len(tasks) + 1 and all(r["status"] == "succeeded" for r in w["runs"]))
                terminal = next(t for t in tasks if not any(t["id"] in other["dependencies"] for other in tasks))
                run = next(r for r in workflow["runs"] if r["taskId"] == terminal["id"])
                command({"type": "test", "id": identifier, "run_id": run["id"]})
                wait(lambda w: w["runs"][-1]["status"] == "succeeded")
                command({"type": "review", "id": identifier, "run_id": run["id"], "decision": "approved"})
                exported = invoke("ai_export", {"workflowId": identifier, "runId": run["id"], "destination": str(root)})
                assert (Path(exported) / "math.test.cjs").is_file()
                assert (project / "math.cjs").read_text() == "module.exports = {};\n"
                request("POST", prefix + "/execute/sync", {"script": "document.querySelector('[aria-label=\"打开代码协作\"]').click();", "args": []})
                time.sleep(1)
                request("POST", prefix + "/execute/sync", {"script": "const s=document.querySelector('[aria-label=\"协作流程\"]'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(s,arguments[0]); s.dispatchEvent(new Event('change',{bubbles:true}));", "args": [identifier]})
                time.sleep(0.5)
                if len(sys.argv) > 2:
                    Path(sys.argv[2]).write_bytes(base64.b64decode(request("GET", prefix + "/screenshot")))
                invoke("ai_shutdown")
                print(f"PASS: native IPC + embedded Pi, {len(tasks)} tasks, generated tests, review/export; original source unchanged")
            finally:
                if session:
                    request("DELETE", f"/session/{session}")
                driver.terminate()
                driver.wait(timeout=10)


if __name__ == "__main__":
    main()
