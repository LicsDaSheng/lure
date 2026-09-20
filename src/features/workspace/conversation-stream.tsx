import { MessagePrimitive, ThreadPrimitive } from "@assistant-ui/react";
import { useMemo } from "react";
import { ScrollTextIcon } from "lucide-react";

import { Reasoning, ReasoningContent, ReasoningTrigger } from "@/components/ai-elements/reasoning";
import { Tool, ToolContent, ToolHeader, ToolInput, ToolOutput } from "@/components/ai-elements/tool";
import { Button } from "@/components/ui/button";
import type { ConversationMessage, ToolPart } from "@/features/pi-connection/reducer";

import { ContentPreviewDialog } from "./content-preview-dialog";
import { MarkdownResponse } from "./markdown-response";
import { createResultDescriptor, formatToolSummary, getToolOutputLineCount } from "./presentation";
import { ResultCard } from "./result-card";

function getToolState(tool: ToolPart) {
  return tool.status === "running"
    ? "input-available"
    : tool.status === "error"
      ? "output-error"
      : "output-available";
}

function ToolItem({ tool }: { tool: ToolPart }) {
  const summary = formatToolSummary(tool);
  const totalLines = getToolOutputLineCount(tool);
  const result = createResultDescriptor(tool);

  return (
    <>
      <Tool defaultOpen={false} status={tool.status}>
        <ToolHeader
          state={getToolState(tool)}
          title={summary}
          toolName={tool.name}
          type="dynamic-tool"
        />
        <ToolContent>
          {tool.input && <ToolInput input={tool.input} />}
          {tool.output && (
            <ToolOutput
              errorText={tool.status === "error" ? tool.output : undefined}
              output={tool.status === "error" ? undefined : tool.output}
            />
          )}
          {tool.output && totalLines > 10 && (
            <ContentPreviewDialog
              content={tool.output}
              description={`${summary} · 共 ${totalLines} 行`}
              title={summary}
              trigger={
                <Button size="xs" type="button" variant="outline">
                  <ScrollTextIcon />
                  查看完整 {totalLines} 行输出
                </Button>
              }
            />
          )}
        </ToolContent>
      </Tool>
      {result && <ResultCard result={result} />}
    </>
  );
}

function StopReason({ message }: { message: ConversationMessage }) {
  if (message.errorMessage) {
    return <p className="text-sm text-[var(--pi-error)]">这次执行未能完成：{message.errorMessage}</p>;
  }
  if (message.stopReason === "length") {
    return (
      <p className="text-sm text-[var(--pi-warning)]">响应在完成前被截断，可以继续追问以补全结果。</p>
    );
  }
  if (message.stopReason === "aborted") {
    return <p className="text-sm text-muted-foreground">任务已由你停止，已完成的内容仍然保留。</p>;
  }
  return null;
}

function PiMessage({
  role,
  source,
}: {
  role: "assistant" | "user" | "system";
  source: ConversationMessage | undefined;
}) {
  return (
    <MessagePrimitive.Root
      aria-label={role === "user" ? "用户消息" : "Pi 回复"}
      className={`group flex w-full flex-col gap-2 ${role === "user" ? "is-user justify-end" : "is-assistant"}`}
      data-role={role}
    >
      <div
        className={
          role === "user"
            ? "flex min-w-0 w-full flex-col gap-2 overflow-hidden bg-[var(--pi-user-bg)] px-4 py-2 text-sm text-foreground"
            : "flex min-w-0 w-full flex-col gap-2 overflow-hidden px-4 py-1 text-sm text-foreground"
        }
      >
        <MessagePrimitive.Parts>
          {({ part }) => {
            if (part.type === "reasoning") {
              return (
                <Reasoning defaultOpen={false} isStreaming={part.status?.type === "running"}>
                  <ReasoningTrigger
                    getThinkingMessage={(streaming, duration) =>
                      streaming
                        ? "思考中…"
                        : duration === undefined
                          ? "思考过程"
                          : `已思考 ${duration} 秒`
                    }
                  />
                  <ReasoningContent>{part.text}</ReasoningContent>
                </Reasoning>
              );
            }
            if (part.type === "text") {
              return <MarkdownResponse>{part.text}</MarkdownResponse>;
            }
            if (part.type === "tool-call") {
              const tool = source?.parts.find(
                (item): item is ToolPart =>
                  item.type === "tool" && item.toolCallId === part.toolCallId,
              );
              return tool ? <ToolItem tool={tool} /> : null;
            }
            return null;
          }}
        </MessagePrimitive.Parts>
        {source && <StopReason message={source} />}
      </div>
    </MessagePrimitive.Root>
  );
}

export function ConversationStream({ messages }: { messages: ConversationMessage[] }) {
  const sourceMessages = useMemo(
    () => new Map(messages.map((message) => [message.id, message])),
    [messages],
  );

  return (
    <ThreadPrimitive.Messages>
      {({ message }) => (
        <PiMessage
          role={message.role}
          source={sourceMessages.get(message.id)}
        />
      )}
    </ThreadPrimitive.Messages>
  );
}
