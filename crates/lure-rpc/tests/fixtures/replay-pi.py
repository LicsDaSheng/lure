#!/usr/bin/env python3
"""按真机采集回放的 Pi RPC 假进程。

读取 `convert-capture.py` 生成的 mock 会话定义，对 Lure 的每个 RPC 请求回放真机
录制里对应的响应，并在 prompt 之后按原始顺序推送该次运行的事件流。

用法：
    LURE_PI_PATH=<此脚本> LURE_REPLAY_FILE=<mock 名称或路径> pnpm tauri dev

环境变量：
    LURE_REPLAY_FILE       mock 文件名（mock-sessions 下）或路径；缺省时要求目录内只有一个
    LURE_REPLAY_DELAY_MS   事件行之间的间隔毫秒，默认 1（设为 0 尽快回放）

mock 只覆盖录制到的内容：录制里没有响应或事件的请求不会凭空生成事件；
超出录制次数的 prompt 只返回响应，并在 stderr 说明。
"""

import gzip
import json
import os
import sys
import threading
import time

FIXTURES = os.path.dirname(os.path.abspath(__file__))
MOCK_DIRECTORY = os.path.join(FIXTURES, "mock-sessions")

HANDSHAKE_COMMANDS = ("get_state", "get_available_models", "get_commands", "set_model")


def fail(message: str, code: int = 2) -> "None":
    print(message, file=sys.stderr)
    raise SystemExit(code)


def mock_path() -> str:
    requested = os.environ.get("LURE_REPLAY_FILE")
    if requested:
        if os.path.isabs(requested) or os.sep in requested:
            return requested
        return os.path.join(MOCK_DIRECTORY, requested)
    if not os.path.isdir(MOCK_DIRECTORY):
        fail(f"缺少 mock 会话目录：{MOCK_DIRECTORY}")
    candidates = sorted(
        entry for entry in os.listdir(MOCK_DIRECTORY) if entry.endswith((".json", ".json.gz"))
    )
    if not candidates:
        fail(f"{MOCK_DIRECTORY} 下没有 mock 会话")
    print(
        f"[replay] 使用 mock：{candidates[0]}（可用 LURE_REPLAY_FILE 指定其它）",
        file=sys.stderr,
    )
    return os.path.join(MOCK_DIRECTORY, candidates[0])


def load_mock(path: str) -> dict:
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt", encoding="utf-8") as handle:
        return json.load(handle)


class Replay:
    def __init__(self, mock: dict, delay_ms: float):
        self.mock = mock
        self.delay = delay_ms / 1000
        self.write_lock = threading.Lock()
        self.cancelled = threading.Event()
        self.threads: list[threading.Thread] = []
        self.prompts = 0
        self.new_sessions = 0

    # 输出

    def send_raw(self, line: str) -> None:
        with self.write_lock:
            sys.stdout.write(line + "\n")
            sys.stdout.flush()

    def send_response(self, request_id: str, command: str, data=..., success: bool = True) -> None:
        body = {
            "id": request_id,
            "type": "response",
            "command": command,
            "success": success,
        }
        if data is not ...:
            body["data"] = data
        self.send_raw(json.dumps(body, ensure_ascii=False, separators=(",", ":")))

    def replay_line(self, request_id: str, recorded: str) -> None:
        """回放录制的原始行，只把请求 id 换成当前请求。"""
        body = json.loads(recorded)
        body["id"] = request_id
        self.send_raw(json.dumps(body, ensure_ascii=False, separators=(",", ":")))

    def replay_response(self, request_id: str, command: str) -> bool:
        """回放录制的命令响应；录制里没有该命令时返回 False。"""
        recorded = self.mock["handshake"].get(command)
        if recorded is None:
            return False
        self.replay_line(request_id, recorded)
        return True

    # 事件推送

    def push(self, lines: list[str], label: str) -> None:
        def run() -> None:
            for line in lines:
                if self.cancelled.is_set():
                    print(f"[replay] {label} 的后续事件被取消", file=sys.stderr)
                    return
                self.send_raw(line)
                if self.delay:
                    time.sleep(self.delay)

        thread = threading.Thread(target=run, daemon=True)
        self.threads.append(thread)
        thread.start()

    def drain(self) -> None:
        """Lure 关闭 stdin 后停掉仍在推送的线程，避免解释器关闭时写 stdout。"""
        self.cancelled.set()
        for thread in self.threads:
            thread.join()

    # 命令处理

    def handle(self, command: dict) -> None:
        command_type = command.get("type")
        request_id = command.get("id")

        if command_type == "extension_ui_response":
            return
        if command_type == "abort":
            self.cancelled.set()
            self.send_response(request_id, "abort", {"cancelled": True})
            return
        if command_type == "prompt":
            self.start_run(request_id)
            return
        if command_type == "set_thinking_level":
            self.send_response(
                request_id, "set_thinking_level", {"thinkingLevel": command.get("level")}
            )
            return
        if command_type == "new_session":
            self.new_session(request_id)
            return
        if command_type == "get_state":
            self.get_state(request_id)
            return
        if request_id is None:
            return
        if self.replay_response(request_id, command_type):
            return
        print(f"[replay] 录制里没有 {command_type} 响应，返回最小成功响应", file=sys.stderr)
        self.send_response(request_id, command_type, {})

    def start_run(self, request_id: str) -> None:
        runs = self.mock["runs"]
        run = runs[self.prompts] if self.prompts < len(runs) else None
        if run is None:
            print(
                f"[replay] 录制只有 {len(runs)} 次 prompt，本次只返回响应，不推送事件",
                file=sys.stderr,
            )
            self.send_response(request_id, "prompt")
            return

        self.prompts += 1
        self.replay_line(request_id, run["prompt"])
        self.cancelled.clear()
        if run["events"]:
            self.push(run["events"], f"prompt#{self.prompts}")

    def recorded_state(self) -> dict:
        """录制里的会话状态；新建会话后按次数给出可区分的合成状态。"""
        state = json.loads(self.mock["handshake"]["get_state"])["data"]
        if not self.new_sessions:
            return state
        suffix = f"-mock-{self.new_sessions}"
        state["sessionId"] = f"{state.get('sessionId', 'replay')}{suffix}"
        session_file = state.get("sessionFile")
        if isinstance(session_file, str):
            state["sessionFile"] = session_file.replace(".jsonl", f"{suffix}.jsonl")
        state["messageCount"] = 0
        state["pendingMessageCount"] = 0
        state["isStreaming"] = False
        return state

    def get_state(self, request_id: str) -> None:
        if self.new_sessions:
            self.send_response(request_id, "get_state", self.recorded_state())
            return
        self.replay_response(request_id, "get_state")

    def new_session(self, request_id: str) -> None:
        self.new_sessions += 1
        self.send_response(request_id, "new_session", {"cancelled": False})


def main() -> None:
    if "--version" in sys.argv:
        print("replay-0.0.0")
        raise SystemExit(0)
    if sys.argv[1:] != ["--mode", "rpc"]:
        fail("unexpected arguments", 2)

    path = mock_path()
    mock = load_mock(path)
    delay_ms = float(os.environ.get("LURE_REPLAY_DELAY_MS", "1"))
    replay = Replay(mock, delay_ms)
    print(
        f"[replay] {mock['name']}（{mock['source']['lines']} 行采集，"
        f"{len(mock['runs'])} 次 prompt，事件间隔 {delay_ms:g}ms）",
        file=sys.stderr,
    )

    if mock["startupEvents"]:
        replay.push(mock["startupEvents"], "启动")

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            command = json.loads(raw_line)
        except json.JSONDecodeError as error:
            fail(f"无法解析 Lure 请求：{error}", 3)
        replay.handle(command)

    replay.drain()


if __name__ == "__main__":
    main()