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

session_number = 1
switched_path = None
queue = {}

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
                "sessionFile": switched_path
                if switched_path
                else os.path.join(
                    os.getcwd(),
                    "session.jsonl" if session_number == 1 else f"session-{session_number}.jsonl",
                ),
                "sessionId": "switched-session"
                if switched_path
                else ("fake-session" if session_number == 1 else f"fake-session-{session_number}"),
                "autoCompactionEnabled": True,
                "messageCount": 0,
                "pendingMessageCount": 0,
            },
        }), flush=True)
    elif command_type == "new_session":
        session_number += 1
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": command_type,
            "success": True,
            "data": {"cancelled": False},
        }), flush=True)
    elif command_type == "switch_session":
        path = command["sessionPath"]
        if "cancelled" in path:
            print(json.dumps({
                "id": request_id,
                "type": "response",
                "command": command_type,
                "success": True,
                "data": {"cancelled": True},
            }), flush=True)
        else:
            switched_path = path
            print(json.dumps({
                "id": request_id,
                "type": "response",
                "command": command_type,
                "success": True,
                "data": {"cancelled": False},
            }), flush=True)
    elif command_type == "get_entries":
        since = command.get("since")
        recorded = [
            {"type": "message", "id": "entry-1", "parentId": None, "message": {"role": "user", "content": "历史提问"}},
            {"type": "message", "id": "entry-2", "parentId": "entry-1", "message": {"role": "assistant", "content": [{"type": "text", "text": "历史回复"}]}},
        ]
        if since is not None and since not in {entry["id"] for entry in recorded}:
            print(json.dumps({"id": request_id, "type": "response", "command": command_type, "success": False, "error": f"Entry not found: {since}"}), flush=True)
        else:
            entries = recorded if since is None else recorded[[entry["id"] for entry in recorded].index(since) + 1:]
            print(json.dumps({
                "id": request_id,
                "type": "response",
                "command": command_type,
                "success": True,
                "data": {"entries": entries, "leafId": "entry-2"},
            }), flush=True)
    elif command_type == "get_available_models":
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": command_type,
            "success": True,
            "data": {"models": [
                {"provider": "test", "id": "fake-model"},
                {"provider": "test", "id": "other-model"},
            ]},
        }), flush=True)
    elif command_type == "set_model":
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": command_type,
            "success": True,
            "data": {"model": {
                "provider": command["provider"],
                "id": command["modelId"],
            }},
        }), flush=True)
    elif command_type == "set_thinking_level":
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": command_type,
            "success": True,
            "data": {"thinkingLevel": command["level"]},
        }), flush=True)
    elif command_type == "get_commands":
        print(json.dumps({
            "id": request_id,
            "type": "response",
            "command": command_type,
            "success": True,
            "data": {"commands": [
                {"name": "review", "description": "Review changes", "source": "extension"},
            ]},
        }), flush=True)
    elif command_type == "prompt":
        if command["message"] == "images" and not command.get("images"):
            print(json.dumps({"id": request_id, "type": "response", "command": "prompt", "success": False, "error": "images missing"}), flush=True)
            continue
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
        print(json.dumps({"type": "message_start", "message": {"role": "user", "content": [{"type": "text", "text": command["message"]}], "timestamp": 0}}), flush=True)
        print(json.dumps({"type": "message_end", "message": {"role": "user", "content": [{"type": "text", "text": command["message"]}], "timestamp": 0}}), flush=True)
        print(json.dumps({"type": "agent_start"}), flush=True)
        print(json.dumps({"type": "turn_start"}), flush=True)
        print(json.dumps({"type": "message_start", "message": {"role": "assistant", "content": [], "timestamp": 1}}), flush=True)
        print(json.dumps({"type": "message_update", "usage": {}, "assistantMessageEvent": {"type": "thinking_delta", "contentIndex": 0, "delta": "思考"}}), flush=True)
        print(json.dumps({"type": "message_update", "usage": {}, "assistantMessageEvent": {"type": "text_delta", "contentIndex": 1, "delta": "RPC_OK"}}), flush=True)
        print(json.dumps({"type": "tool_execution_start", "toolCallId": "tool-1", "toolName": "read", "args": {}}), flush=True)
        print(json.dumps({"type": "tool_execution_update", "toolCallId": "tool-1", "toolName": "read", "args": {}, "partialResult": {}}), flush=True)
        print(json.dumps({"type": "tool_execution_end", "toolCallId": "tool-1", "toolName": "read", "result": {}, "isError": False}), flush=True)
        print(json.dumps({"type": "message_end", "message": {"role": "assistant", "content": [{"type": "thinking", "thinking": "思考"}, {"type": "text", "text": "RPC_OK"}], "stopReason": "stop", "timestamp": 2}}), flush=True)
        print(json.dumps({"type": "turn_end", "message": {"role": "assistant", "stopReason": "stop"}, "toolResults": []}), flush=True)
        print(json.dumps({"type": "agent_end", "messages": [], "willRetry": False}), flush=True)
        print(json.dumps({"type": "agent_settled"}), flush=True)
    elif command_type in ("steer", "follow_up"):
        if command["message"] == "extension":
            print(json.dumps({"id": request_id, "type": "response", "command": command_type, "success": False, "error": "extension commands are not supported"}), flush=True)
            continue
        queue.setdefault(command_type, []).append(command["message"])
        print(json.dumps({"id": request_id, "type": "response", "command": command_type, "success": True}), flush=True)
        print(json.dumps({"type": "queue_update", "steering": queue.get("steer", []), "followUp": queue.get("follow_up", [])}), flush=True)
    elif command_type == "clear_queue":
        cleared = {"steering": queue.get("steer", []), "followUp": queue.get("follow_up", [])}
        queue.clear()
        print(json.dumps({"id": request_id, "type": "response", "command": "clear_queue", "success": True, "data": cleared}), flush=True)
        print(json.dumps({"type": "queue_update", "steering": [], "followUp": []}), flush=True)
    elif command_type == "abort":
        print(json.dumps({"id": request_id, "type": "response", "command": "abort", "success": True}), flush=True)
    elif command_type == "extension_ui_response":
        if command.get("id") == "ui-1":
            print(json.dumps({"type": "extension_ui_request", "id": "notice-1", "method": "notify", "message": "response-received", "notifyType": "info"}), flush=True)
        continue
    else:
        print(json.dumps({"id": request_id, "type": "response", "command": command_type, "success": False, "error": "unsupported"}), flush=True)
