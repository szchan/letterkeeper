# SPDX-License-Identifier: GPL-3.0-only
"""Exercise the installed developer graph: stdio MCP and loopback Web UI."""
import json
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
LAUNCH = [sys.executable, str(ROOT / "tools/gitnexus.py")]


def main():
    with tempfile.TemporaryFile() as log:
        mcp = subprocess.Popen([*LAUNCH, "mcp"], cwd=ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log)
        selector = selectors.DefaultSelector()
        selector.register(mcp.stdout, selectors.EVENT_READ)
        buffer = b""

        def rpc(request_id, method, params):
            nonlocal buffer
            message = {"jsonrpc": "2.0", "method": method, "params": params}
            if request_id is not None:
                message["id"] = request_id
            mcp.stdin.write(json.dumps(message).encode() + b"\n")
            mcp.stdin.flush()
            if request_id is None:
                return
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    result = json.loads(line)
                    if result.get("id") == request_id:
                        return result
                if selector.select(timeout=min(1, max(0, deadline - time.monotonic()))):
                    chunk = mcp.stdout.read1(65536)
                    if not chunk:
                        raise RuntimeError("MCP exited before response")
                    buffer += chunk
            raise TimeoutError(f"MCP method timed out: {method}")

        try:
            response = rpc(1, "initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "letterkeeper-graph-test", "version": "1"}})
            assert "result" in response, response
            rpc(None, "notifications/initialized", {})
            response = rpc(2, "tools/list", {})
            names = {tool["name"] for tool in response["result"]["tools"]}
            assert {"query", "context", "impact", "trace", "list_repos"} <= names
            assert not {"rename", "cypher", "group_sync", "group_list", "read_file", "grep"} & names
            response = rpc(3, "tools/call", {"name": "query", "arguments": {"query": "parse_message", "repo": "letterkeeper"}})
            assert "error" not in response and not response["result"].get("isError"), response
            assert "crates/letterkeeper-content/src/mime.rs" in json.dumps(response), response
            response = rpc(4, "tools/call", {"name": "context", "arguments": {"name": "parse_message", "repo": "letterkeeper"}})
            assert "error" not in response and not response["result"].get("isError"), response
            assert "crates/letterkeeper-content/src/mime.rs" in json.dumps(response), response
            for request_id, name in [(5, "rename"), (6, "cypher")]:
                response = rpc(request_id, "tools/call", {"name": name, "arguments": {"repo": "letterkeeper"}})
                assert "error" in response or response["result"].get("isError"), response
            print("MCP initialize, discovery, keyword query, symbol context and write/raw-query denial: passed")
        finally:
            selector.close()
            mcp.terminate()
            try:
                mcp.wait(timeout=10)
            except subprocess.TimeoutExpired:
                mcp.kill()
                mcp.wait()

        server = subprocess.Popen([*LAUNCH, "serve"], cwd=ROOT, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 30
            while True:
                try:
                    with urllib.request.urlopen("http://127.0.0.1:4747/api/health", timeout=1) as reply:
                        assert reply.status == 200
                    break
                except OSError:
                    if server.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("loopback graph server did not start")
            with urllib.request.urlopen("http://127.0.0.1:4747/", timeout=10) as reply:
                assert b"/assets/" in reply.read(), "packaged graph UI missing"
            with urllib.request.urlopen("http://127.0.0.1:4747/api/repos", timeout=10) as reply:
                assert "letterkeeper" in reply.read().decode()
            with urllib.request.urlopen("http://127.0.0.1:4747/api/graph?repo=letterkeeper", timeout=30) as reply:
                graph = json.load(reply)
                assert graph["nodes"] and graph["edges"], "empty graph"
            print("Loopback HTTP health, packaged Web UI, repository list and nonempty graph: passed")
        finally:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()


if __name__ == "__main__":
    main()
