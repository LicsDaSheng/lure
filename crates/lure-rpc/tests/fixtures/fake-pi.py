#!/usr/bin/env python3
import json
import os
import sys
import threading

if "--version" in sys.argv:
    print("0.0.0-test")
    raise SystemExit(0)

if sys.argv[1:] != ["--mode", "rpc"]:
    print("unexpected arguments", file=sys.stderr)
    raise SystemExit(2)

for raw_line in sys.stdin:
    command = json.loads(raw_line)
    request_id = command.get("id")
    command_type = command["type"]

    if command_type == "get_state":
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": "get_state",
            "success": True,
            "data": {
                "model": {"provider": "test", "id": "fake-model"},
                "thinkingLevel": "medium",
                "isStreaming": False,
                "isCompacting": False,
                "steeringMode": "one-at-a-time",
                "followUpMode": "one-at-a-time",
                "sessionFile": os.path.join(os.getcwd(), "session.jsonl"),
                "sessionId": "fake-session",
                "autoCompactionEnabled": True,
                "messageCount": 0,
                "pendingMessageCount": 0,
            },
        }), flush=True)
    elif command_type == "prompt":
        if command["message"] == "out-of-order":
            threading.Timer(0.1, lambda rid=request_id: print(json.dumps({"id": rid, "type": "response", "command": "prompt", "success": True}), flush=True)).start()
            continue
        if command["message"] == "never-respond":
            continue
        print(json.dumps({"id": request_id, "type": "response", "command": "prompt", "success": True}), flush=True)
        if command["message"] == "stderr":
            print("not-json-from-stderr", file=sys.stderr, flush=True)
            continue
        if command["message"] == "extension":
            print(json.dumps({"type": "extension_ui_request", "id": "ui-1", "method": "confirm", "title": "确认", "message": "继续吗"}), flush=True)
            continue
        print(json.dumps({"type": "agent_start"}), flush=True)
        print(json.dumps({"type": "message_start", "message": {"role": "assistant", "content": [], "timestamp": 1}}), flush=True)
        print(json.dumps({"type": "message_update", "usage": {}, "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "思考"}}), flush=True)
        print(json.dumps({"type": "message_update", "usage": {}, "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "RPC_OK"}}), flush=True)
        print(json.dumps({"type": "tool_execution_start", "toolCallId": "tool-1", "toolName": "read", "args": {}}), flush=True)
        print(json.dumps({"type": "tool_execution_update", "toolCallId": "tool-1", "toolName": "read", "args": {}, "partialResult": {}}), flush=True)
        print(json.dumps({"type": "tool_execution_end", "toolCallId": "tool-1", "toolName": "read", "result": {}, "isError": False}), flush=True)
        print(json.dumps({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "thinking", "thinking": "思考"}, {"type": "text", "text": "RPC_OK"}], "timestamp": 2}}), flush=True)
        print(json.dumps({"type": "agent_end", "messages": [], "willRetry": False}), flush=True)
        print(json.dumps({"type": "agent_settled"}), flush=True)
    elif command_type == "abort":
        print(json.dumps({"id": request_id, "type": "response", "command": "abort", "success": True}), flush=True)
    elif command_type == "extension_ui_response":
        if command.get("id") == "ui-1" and command.get("cancelled") is True:
            print(json.dumps({"type": "extension_ui_request", "id": "notice-1", "method": "notify", "message": "cancelled-received", "notifyType": "info"}), flush=True)
        continue
    else:
        print(json.dumps({"id": request_id, "type": "response", "command": command_type, "success": False, "error": "unsupported"}), flush=True)
