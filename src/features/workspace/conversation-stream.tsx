import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import { Reasoning, ReasoningContent, ReasoningTrigger } from "@/components/ai-elements/reasoning";
import { Tool, ToolContent, ToolHeader, ToolInput, ToolOutput } from "@/components/ai-elements/tool";
import { Button } from "@/components/ui/button";
import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";
import { ScrollTextIcon } from "lucide-react";

import { ContentPreviewDialog } from "./content-preview-dialog";
import { createResultDescriptor, formatToolSummary, getToolOutputLineCount } from "./presentation";
import { ResultCard } from "./result-card";

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