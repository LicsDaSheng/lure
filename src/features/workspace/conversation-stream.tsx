import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import { Reasoning, ReasoningContent, ReasoningTrigger } from "@/components/ai-elements/reasoning";
import { Tool, ToolContent, ToolHeader, ToolInput, ToolOutput } from "@/components/ai-elements/tool";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";
import { ScrollTextIcon } from "lucide-react";

import { ContentPreviewDialog } from "./content-preview-dialog";
import { createResultDescriptor, formatToolSummary, getToolOutputLineCount } from "./presentation";
import { ResultCard } from "./result-card";
import { RunStatusRecord } from "./run-status-line";

function getToolState(tool: ToolRun) {
  return tool.status === "running"
    ? "input-available"
    : tool.status === "error"
      ? "output-error"
      : "output-available";
}

function ToolItem({ tool }: { tool: ToolRun }) {
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

function SystemMessage({ message }: { message: ConversationMessage }) {
  if (message.kind === "compaction") {
    return (
      <details className="rounded-xl border border-border/60 bg-muted/25 px-4 py-2 text-sm text-muted-foreground">
        <summary className="cs-smooth-interaction-fast cursor-pointer list-none outline-none">
          <Badge className="mr-2 rounded-md" variant="outline">
            上下文压缩
          </Badge>
          {message.pending
            ? "正在压缩上下文…"
            : message.errorMessage
              ? `压缩未完成：${message.errorMessage}`
              : `已压缩上下文${message.tokensBefore ? `（压缩前 ${message.tokensBefore} tokens）` : ""}，点击查看摘要`}
        </summary>
        {message.content && <MessageResponse className="mt-2">{message.content}</MessageResponse>}
      </details>
    );
  }
  return (
    <div className="rounded-xl border border-border/60 bg-muted/25 px-4 py-2 text-sm text-muted-foreground">
      <Badge className="mr-2 rounded-md" variant="outline">
        分支摘要
      </Badge>
      {message.content}
    </div>
  );
}

export function ConversationStream({
  messages,
  isRunning,
  activeAssistantId,
}: {
  messages: ConversationMessage[];
  isRunning: boolean;
  activeAssistantId: string | null;
}) {
  return (
    <>
      {messages.map((message) => {
        if (message.kind === "status") {
          return (
            <RunStatusRecord
              content={message.content}
              key={message.id}
              status={message.status ?? "idle"}
            />
          );
        }
        if (message.role === "system") {
          return (
            <Message from="system" key={message.id}>
              <SystemMessage message={message} />
            </Message>
          );
        }
        return (
          <Message from={message.role} key={message.id}>
            <MessageContent>
              {message.blocks.map((block) =>
                block.type === "thinking" ? (
                  <Reasoning
                    defaultOpen={false}
                    isStreaming={isRunning && activeAssistantId === message.id}
                    key={`thinking-${block.contentIndex}`}
                  >
                    <ReasoningTrigger
                      getThinkingMessage={(streaming, duration) =>
                        streaming
                          ? "思考中…"
                          : duration === undefined
                            ? "思考过程"
                            : `已思考 ${duration} 秒`
                      }
                    />
                    <ReasoningContent>{block.text}</ReasoningContent>
                  </Reasoning>
                ) : (
                  <MessageResponse key={`text-${block.contentIndex}`}>{block.text}</MessageResponse>
                ),
              )}
              {message.tools.map((tool) => (
                <ToolItem key={tool.id} tool={tool} />
              ))}
              <StopReason message={message} />
            </MessageContent>
          </Message>
        );
      })}
    </>
  );
}