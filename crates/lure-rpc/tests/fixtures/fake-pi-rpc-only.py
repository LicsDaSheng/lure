#!/usr/bin/env python3
import json
import sys

if sys.argv[1:] != ["--mode", "rpc"]:
    print("unexpected arguments", file=sys.stderr)
    raise SystemExit(2)

for raw_line in sys.stdin:
    command = json.loads(raw_line)
    if command["type"] == "get_state":
        print(json.dumps({
            "id": command["id"],
            "type": "response",
            "command": "get_state",
            "success": True,
            "data": {
                "model": {"provider": "test", "id": "rpc-only-model"},
                "thinkingLevel": "medium",
                "isStreaming": False,
                "isCompacting": False,
                "steeringMode": "one-at-a-time",
                "followUpMode": "one-at-a-time",
                "sessionId": "rpc-only-session",
                "sessionFile": None,
                "sessionName": None,
                "autoCompactionEnabled": True,
                "messageCount": 0,
                "pendingMessageCount": 0,
            },
        }), flush=True)
