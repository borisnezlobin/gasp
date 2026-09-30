"""Plays an agent: starts `gasp mcp <vault>` and sends real tools/call
requests over stdio, as an MCP client does.

    python mcp_call.py <vault> <calls.json> <transcript.json> [--wait]

With --wait it prints "ready" once the server is initialized and sends the
calls when a line arrives on stdin.

calls.json is a list of {"name": ..., "arguments": {...}, "pause": s}.
The transcript records each request and response with wall-clock times,
which the reel's terminal pane replays verbatim.
"""
import json
import os
import subprocess
import sys
import time


def main() -> None:
    vault, calls_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    calls = json.load(open(calls_path))
    gasp = os.environ.get("GASP", "/tmp/target-app/release/gasp")
    server = subprocess.Popen(
        [gasp, "mcp", vault], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    log = []

    def send(message: dict, expect_reply: bool = True):
        line = json.dumps(message)
        log.append({"wall": time.time(), "dir": "out", "text": line})
        server.stdin.write(line + "\n")
        server.stdin.flush()
        if not expect_reply:
            return None
        reply = server.stdout.readline()
        log.append({"wall": time.time(), "dir": "in", "text": reply.strip()})
        return json.loads(reply)

    send({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "reel-agent", "version": "1.0"}}})
    send({"jsonrpc": "2.0", "method": "notifications/initialized"}, expect_reply=False)
    if "--wait" in sys.argv:
        print("ready", flush=True)
        sys.stdin.readline()
    for index, call in enumerate(calls, start=1):
        time.sleep(call.get("pause", 0))
        reply = send({"jsonrpc": "2.0", "id": index, "method": "tools/call",
                      "params": {"name": call["name"], "arguments": call["arguments"]}})
        print(json.dumps(reply)[:300], file=sys.stderr)
    server.stdin.close()
    server.wait(timeout=10)
    json.dump(log, open(out_path, "w"), indent=1)


if __name__ == "__main__":
    main()
