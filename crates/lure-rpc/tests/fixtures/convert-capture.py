#!/usr/bin/env python3
"""把真机采集的 Pi RPC stdout 原始行转换成可回放的 mock 会话。

输入是 Lure 临时采集功能写出的 `stdout-<UTC>.jsonl`（Pi 子进程 stdout 的原始行），
输出是 `replay-pi.py` 消费的 mock 会话定义。转换只做分段与索引，行内容原样保留。

用法：
    convert-capture.py <stdout.jsonl> --name <mock 名称> [--output <路径>] [--plain]

默认输出到 fixtures/mock-sessions/<名称>.json.gz（gzip 且固定 mtime，便于复现）；
`--plain` 输出未压缩 JSON，便于人工检查。
"""

import argparse
import gzip
import json
import os
import sys

FIXTURES = os.path.dirname(os.path.abspath(__file__))
DEFAULT_DIRECTORY = os.path.join(FIXTURES, "mock-sessions")

PROMPT = "prompt"


def parse_lines(raw: bytes) -> list[str]:
    """按 JSONL 边界切分，保留每行原始字节（去掉行尾换行）。"""
    lines = []
    for chunk in raw.split(b"\n"):
        if chunk.strip():
            lines.append(chunk.decode("utf-8"))
        else:
            assert not chunk, "采集文件出现了非空白的无换行尾部"
    return lines


def build_mock(lines: list[str], name: str, capture_name: str, size: int) -> dict:
    handshake: dict[str, object] = {}
    startup_events: list[str] = []
    runs: list[dict[str, object]] = []
    current_events: list[str] | None = None

    for index, line in enumerate(lines):
        record = json.loads(line)
        if record.get("type") == "response" and record.get("command") == PROMPT:
            current_events = []
            runs.append({"prompt": line, "events": current_events})
            continue
        if current_events is not None:
            current_events.append(line)
            continue
        if record.get("type") == "response":
            command = record["command"]
            if command in handshake:
                raise SystemExit(f"命令 {command} 出现多个握手响应，无法确定回放哪一个")
            handshake[command] = line
            continue
        startup_events.append(line)

    counted = len(handshake) + len(startup_events) + sum(
        1 + len(run["events"]) for run in runs
    )
    if counted != len(lines):
        raise SystemExit(f"转换丢失了行：输入 {len(lines)}，分发 {counted}")

    startup_index = len(handshake) + len(startup_events)
    if startup_index >= len(lines) or not lines[startup_index].lstrip().startswith("{"):
        raise SystemExit("首个 prompt 响应之后的分段不正确")

    return {
        "version": 1,
        "name": name,
        "source": {
            "capture": capture_name,
            "lines": len(lines),
            "bytes": size,
        },
        "handshake": handshake,
        "startupEvents": startup_events,
        "runs": runs,
    }


def write(output: str, mock: dict, plain: bool) -> None:
    os.makedirs(os.path.dirname(output), exist_ok=True)
    payload = json.dumps(mock, ensure_ascii=False, separators=(",", ":"))
    if plain:
        with open(output, "w", encoding="utf-8") as handle:
            handle.write(payload)
        return
    # mtime=0 固定 gzip 头，保证同一份采集始终生成相同的字节。
    with open(output, "wb") as raw:
        with gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as handle:
            handle.write(payload.encode("utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", help="采集到的 stdout JSONL 文件")
    parser.add_argument("--name", required=True, help="mock 会话名称")
    parser.add_argument("--output", default=None, help="输出路径，默认 mock-sessions/<名称>.json.gz")
    parser.add_argument("--plain", action="store_true", help="输出未压缩 JSON")
    args = parser.parse_args()

    with open(args.capture, "rb") as handle:
        raw = handle.read()
    lines = parse_lines(raw)
    mock = build_mock(lines, args.name, os.path.basename(args.capture), len(raw))

    default_name = f"{args.name}.json" if args.plain else f"{args.name}.json.gz"
    output = args.output or os.path.join(DEFAULT_DIRECTORY, default_name)
    write(output, mock, args.plain)

    summary = {
        "output": output,
        "lines": len(lines),
        "handshake": sorted(mock["handshake"]),
        "startupEvents": len(mock["startupEvents"]),
        "runs": [len(run["events"]) for run in mock["runs"]],
    }
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    sys.exit(main())