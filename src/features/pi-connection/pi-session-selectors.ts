import { createSelector } from "@reduxjs/toolkit";

import type { RootState } from "@/app/store";

import type {
  ConnectionPhase,
  ConversationMessage,
  MessagePart,
  PiSessionSummary,
  TextPart,
  TurnPhase,
} from "./pi-session-types";

export type ConversationProcessGroup = {
  id: string;
  parts: MessagePart[];
  stopReason: string | null;
  errorMessage: string | null;
  isRunning: boolean;
};

export type ConversationResult = {
  id: string;
  kind: "candidate" | "final" | "partial";
  parts: TextPart[];
  stopReason: string | null;
  errorMessage: string | null;
};

export type ConversationTurn = {
  id: string;
  user: ConversationMessage | null;
  process: ConversationProcessGroup[];
  result: ConversationResult | null;
  isRunning: boolean;
  /** 响应组阶段；运行中为 `running`，否则由 Pi 的终止原因定案。 */
  phase: TurnPhase;
  durationMs: number | null;
  toolCount: number;
};

export const selectPiConnection = (state: RootState) => state.piConnection;
export const selectPiSessionState = createSelector(selectPiConnection, (state) => state);
export const selectConnection = createSelector(selectPiConnection, (state) => state.connection);
export const selectMessages = createSelector(selectPiConnection, (state) => state.messages);

/** Pi 中代表“不能称为最终答案”的终止原因。 */
const INCOMPLETE_STOP_REASONS = new Set(["error", "length", "aborted"]);

function hasText(message: ConversationMessage): boolean {
  return message.parts.some((part) => part.type === "text");
}

function hasTool(message: ConversationMessage): boolean {
  return message.parts.some((part) => part.type === "tool");
}

/** 正常完成：既没有错误，也不是截断或中止。 */
function isNormalStop(message: ConversationMessage): boolean {
  return !message.errorMessage && !INCOMPLETE_STOP_REASONS.has(message.stopReason ?? "");
}

/**
 * 把 Pi 的原始消息事实投影为“用户指令→执行过程→主结果”。
 *
 * 以一次用户指令为一个响应组：思考和工具始终属于过程；带工具调用的消息
 * 整体属于过程；只有纯文本消息能成为候选或最终结果。运行中最后一条纯文本
 * 消息是候选结果，`agent_settled` 后由 Pi 记录的终止原因定案：
 * 正常完成是最终结果，`aborted`/`length`/`error` 是部分结果。
 */
export function conversationTurns(
  messages: ConversationMessage[],
  phase: ConnectionPhase,
  activeAssistantId: string | null,
): ConversationTurn[] {
  const grouped: Array<{ user: ConversationMessage | null; assistants: ConversationMessage[] }> = [];

  for (const message of messages) {
    if (message.role === "user") {
      grouped.push({ user: message, assistants: [] });
      continue;
    }
    const current = grouped.at(-1);
    if (current) current.assistants.push(message);
    else grouped.push({ user: null, assistants: [message] });
  }

  return grouped.map((group, groupIndex) => {
    const lastAssistant = group.assistants.at(-1) ?? null;
    const isRunning = groupIndex === grouped.length - 1 && phase === "running";
    const turnPhase: TurnPhase = isRunning
      ? "running"
      : lastAssistant?.turnPhase ?? "settled";
    const textMessages = group.assistants.filter(hasText);
    // 候选或最终结果必须来自组内最后一条 assistant 消息。
    // 若最后一条进入了工具调用，更早的纯文本也已是执行过程，
    // 不得回退为当前结果。异常终止时例外保留最后消息的部分文本。
    const resultMessage =
      lastAssistant &&
      hasText(lastAssistant) &&
      (!hasTool(lastAssistant) || (!isRunning && !isNormalStop(lastAssistant)))
        ? lastAssistant
        : null;
    const normalResult =
      resultMessage !== null && !hasTool(resultMessage) && isNormalStop(resultMessage);
    const result: ConversationResult | null = resultMessage
      ? {
          id: resultMessage.id,
          kind: isRunning
            ? "candidate"
            : normalResult && turnPhase === "settled"
              ? "final"
              : "partial",
          parts: resultMessage.parts.flatMap<TextPart>((part) =>
            part.type === "text" ? [part] : [],
          ),
          stopReason: resultMessage.stopReason ?? null,
          errorMessage: resultMessage.errorMessage ?? null,
        }
      : // 没有文本内容但被停止、截断或失败时，也要在响应组内如实说明。
        !isRunning && textMessages.length === 0 && turnPhase !== "settled" && lastAssistant
        ? {
            id: lastAssistant.id,
            kind: "partial",
            parts: [],
            stopReason: lastAssistant.stopReason ?? null,
            errorMessage: lastAssistant.errorMessage ?? null,
          }
        : null;

    const process = group.assistants.flatMap<ConversationProcessGroup>((message) => {
      const isResultMessage = message.id === result?.id;
      const parts = isResultMessage
        ? message.parts.filter((part) => part.type !== "text")
        : message.parts;
      const stopReason = isResultMessage ? null : message.stopReason ?? null;
      const errorMessage = isResultMessage ? null : message.errorMessage ?? null;
      if (parts.length === 0 && !stopReason && !errorMessage) return [];
      return [
        {
          id: message.id,
          parts,
          stopReason,
          errorMessage,
          isRunning: isRunning && message.id === activeAssistantId,
        },
      ];
    });

    return {
      id: group.user?.id ?? group.assistants[0]?.id ?? `turn-${groupIndex}`,
      user: group.user,
      process,
      result,
      isRunning,
      phase: turnPhase,
      durationMs: lastAssistant?.runDurationMs ?? null,
      toolCount: group.assistants.reduce(
        (count, message) =>
          count + message.parts.filter((part) => part.type === "tool").length,
        0,
      ),
    };
  });
}

export const selectConversationTurns = createSelector(selectPiConnection, (state) =>
  conversationTurns(state.messages, state.connection.phase, state.activeAssistantId),
);

/** 会话列表按最近活动时间倒序展示；store 只保存 Pi 侧的事实。 */
function byRecentActivity(sessions: PiSessionSummary[]): PiSessionSummary[] {
  return [...sessions].sort((left, right) => right.modifiedAtMs - left.modifiedAtMs);
}

export const selectRecentSessions = createSelector(selectPiConnection, (state) =>
  byRecentActivity(state.recentSessions),
);
export const selectProjectSessions = createSelector(selectPiConnection, (state) =>
  Object.fromEntries(
    Object.entries(state.projectSessions).map(([directory, sessions]) => [
      directory,
      byRecentActivity(sessions),
    ]),
  ),
);
export const selectExpandedProjects = createSelector(
  selectPiConnection,
  (state) => state.expandedProjects,
);
export const selectLoadingDirectories = createSelector(
  selectPiConnection,
  (state) => state.loadingDirectories,
);
export const selectSessionError = createSelector(
  selectPiConnection,
  (state) => state.commandError ?? state.error,
);
export const selectCanSend = createSelector(
  selectConnection,
  (connection) => connection.phase === "ready",
);
export const selectIsRunning = createSelector(
  selectConnection,
  (connection) => connection.phase === "running",
);
