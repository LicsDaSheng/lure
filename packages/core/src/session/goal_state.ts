//! 显式持续目标（goal）的 session metadata 派生视图（对齐 crates/lure-core/src/session/goal_state.rs）。

import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";

export const GOAL_STATE_KEY = "goal_state";
export const GOAL_COMMAND = "/goal";
export const MAX_GOAL_OBJECTIVE_CHARS = 4000;
const LEGACY_GOAL_STATE_SESSION_KEY = "thread_goal";
const MAX_OBJECTIVE_WS = 600;

/// 返回 `goal_state`（或 legacy `thread_goal`）原始 blob。
export function goalStateRaw(metadata?: Record<string, Json>): Json | undefined {
  if (!metadata || Object.keys(metadata).length === 0) return undefined;
  if (metadata[GOAL_STATE_KEY] !== undefined) return metadata[GOAL_STATE_KEY];
  return metadata[LEGACY_GOAL_STATE_SESSION_KEY];
}

/// 移除 legacy metadata key。
export function discardLegacyGoalStateKey(metadata: Record<string, Json>): void {
  delete metadata[LEGACY_GOAL_STATE_SESSION_KEY];
}

/// 把 goal blob 解析为对象；接受对象或 JSON 字符串，其余返回 `undefined`。
export function parseGoalState(blob?: Json): Record<string, Json> | undefined {
  if (blob === undefined) return undefined;
  const obj = asObject(blob);
  if (obj) return obj;
  if (typeof blob === "string") {
    try {
      const parsed = asObject(JSON.parse(blob));
      return parsed;
    } catch {
      return undefined;
    }
  }
  return undefined;
}

/// 该 session 是否存在激活的持续目标。
export function sustainedGoalActive(metadata?: Record<string, Json>): boolean {
  const raw = goalStateRaw(metadata);
  const goal = parseGoalState(raw);
  return goal !== undefined && statusIsActive(goal);
}

/// 本轮是否由 `/goal` 命令显式发起。
export function explicitGoalRequested(messageMetadata?: Record<string, Json>): boolean {
  if (!messageMetadata) return false;
  if (messageMetadata["goal_requested"] === true) return true;
  return (asString(messageMetadata["original_command"]) ?? "").trim() === GOAL_COMMAND;
}

/// 本轮是否应使用持续目标的运行时限制。
export function sustainedGoalTurn(
  metadata?: Record<string, Json>,
  messageMetadata?: Record<string, Json>,
): boolean {
  return sustainedGoalActive(metadata) || explicitGoalRequested(messageMetadata);
}

/// goal 激活时追加到 Runtime Context 块的文本行。
export function goalStateRuntimeLines(metadata?: Record<string, Json>): string[] {
  const goal = parseGoalState(goalStateRaw(metadata));
  if (!goal || !statusIsActive(goal)) return [];

  const objective = strField(goal, "objective");
  if (objective === "") {
    return ["Goal: active (no objective text stored)."];
  }
  const truncated = truncateChars(objective, MAX_GOAL_OBJECTIVE_CHARS, "\n… (truncated)");

  const out = ["Goal (active):", truncated];
  const hint = strField(goal, "ui_summary");
  if (hint !== "") out.push(`Summary: ${hint}`);
  return out;
}

/// WebSocket `goal_state` 事件的 JSON-safe 快照。
export function goalStateWsBlob(metadata?: Record<string, Json>): Json {
  const goal = parseGoalState(goalStateRaw(metadata));
  if (goal && statusIsActive(goal)) {
    let objective = strField(goal, "objective");
    if (objective.length > MAX_OBJECTIVE_WS) {
      objective = truncateChars(objective, MAX_OBJECTIVE_WS, "…");
    }
    const summary = strField(goal, "ui_summary").slice(0, 120);

    const blob: Record<string, Json> = { active: true };
    if (summary !== "") blob["ui_summary"] = summary;
    if (objective !== "") blob["objective"] = objective;
    return blob;
  }
  return { active: false };
}

function statusIsActive(goal: Record<string, Json>): boolean {
  return asString(goal["status"]) === "active";
}

function strField(goal: Record<string, Json>, key: string): string {
  return (asString(goal[key]) ?? "").trim();
}

function truncateChars(text: string, max: number, suffix: string): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max).trimEnd()}${suffix}`;
}
