import { MessagePrimitive, ThreadPrimitive, type ThreadMessage } from "@assistant-ui/react";
import type { ReactNode } from "react";

import { MarkdownResponse } from "./markdown-response";
import { ReasoningPart } from "./reasoning-part";
import { ToolPartCard } from "./tool-part";

function StopReason({ message }: { message: ThreadMessage }): ReactNode {
  const status = message.status;
  if (!status || status.type !== "incomplete") return null;

  if (status.reason === "error") {
    const detail = typeof status.error === "string" ? status.error : null;
    return (
      <p className="text-sm text-[var(--pi-error)]">
        {detail ? `这次执行未能完成：${detail}` : "这次执行未能完成。"}
      </p>
    );
  }
  if (status.reason === "length") {
    return (
      <p className="text-sm text-[var(--pi-warning)]">
        响应在完成前被截断，可以继续追问以补全结果。
      </p>
    );
  }
  if (status.reason === "cancelled") {
    return (
      <p className="text-sm text-muted-foreground">
        任务已由你停止，已完成的内容仍然保留。
      </p>
    );
  }
  return null;
}

function PiMessage({ message }: { message: ThreadMessage }) {
  const role = message.role;
  const isUser = role === "user";

  return (
    <MessagePrimitive.Root
      aria-label={isUser ? "用户消息" : "Pi 回复"}
      className={`group flex w-full flex-col gap-2 ${isUser ? "is-user justify-end" : "is-assistant"}`}
      data-role={role}
    >
      <div
        className={
          isUser
            ? "flex min-w-0 w-full flex-col gap-2 overflow-hidden bg-[var(--pi-user-bg)] px-4 py-2 text-sm text-foreground"
            : "flex min-w-0 w-full flex-col gap-2 overflow-hidden px-4 py-1 text-sm text-foreground"
        }
      >
        <MessagePrimitive.Parts
          components={{
            Text: ({ text }) => <MarkdownResponse>{text}</MarkdownResponse>,
            Reasoning: ReasoningPart,
            tools: { Override: ToolPartCard },
          }}
        />
        {role === "assistant" && <StopReason message={message} />}
      </div>
    </MessagePrimitive.Root>
  );
}

export function ConversationStream() {
  return (
    <ThreadPrimitive.Messages>
      {({ message }) => <PiMessage message={message} />}
    </ThreadPrimitive.Messages>
  );
}