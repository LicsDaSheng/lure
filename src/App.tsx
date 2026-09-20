import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import {
  PromptInput,
  PromptInputBody,
  PromptInputFooter,
  type PromptInputMessage,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
} from "@/components/ai-elements/prompt-input";
import {
  Reasoning,
  ReasoningContent,
  ReasoningTrigger,
} from "@/components/ai-elements/reasoning";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolInput,
  ToolOutput,
} from "@/components/ai-elements/tool";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { usePiSession } from "@/features/pi-connection/use-pi-session";
import {
  BotIcon,
  FolderIcon,
  LinkIcon,
  LoaderCircleIcon,
  UnplugIcon,
} from "lucide-react";
import { useCallback, useState } from "react";
import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";

const phaseLabels = {
  disconnected: "未连接",
  connecting: "正在连接",
  ready: "已连接",
  running: "运行中",
  failed: "连接异常",
} as const;

function getToolState(tool: ToolRun) {
  return tool.status === "running"
    ? "input-available"
    : tool.status === "error"
      ? "output-error"
      : "output-available";
}

function extractCommand(input: string) {
  try {
    const parsed = JSON.parse(input) as { command?: unknown };
    return typeof parsed.command === "string" ? parsed.command : input;
  } catch {
    return input;
  }
}

function BashTool({ tool }: { tool: ToolRun }) {
  const lines = tool.output.split("\n");
  const hiddenLines = Math.max(0, lines.length - 20);
  const exitCode = tool.output.match(/Command exited with code (\d+)/)?.[1];
  const cancelled = /cancelled|aborted/i.test(tool.output);
  return (
    <Tool
      className="border-y border-[var(--pi-success)] bg-transparent"
      defaultOpen={tool.status !== "completed"}
      key={`${tool.id}-${tool.status}`}
      status={tool.status}
    >
      <ToolHeader
        className="font-mono font-bold text-[var(--pi-success)] [&>div>span]:font-bold [&>div>span]:text-[var(--pi-success)]"
        state={getToolState(tool)}
        title={`$ ${extractCommand(tool.input)}`}
        toolName={tool.name}
        type="dynamic-tool"
      />
      <ToolContent>
        {tool.status === "running" && (
          <p className="flex items-center gap-2 text-sm text-[var(--pi-muted)]">
            <LoaderCircleIcon className="size-4 animate-spin" /> Running… (key to cancel)
          </p>
        )}
        {tool.output && (
          <pre className="whitespace-pre-wrap bg-muted/30 p-2 text-sm text-[var(--pi-muted)]">
            {lines.slice(-20).join("\n")}
          </pre>
        )}
        {exitCode && <p className="text-xs text-[var(--pi-error)]">(exit {exitCode})</p>}
        {cancelled && <p className="text-xs text-[var(--pi-warning)]">(cancelled)</p>}
        {(hiddenLines > 0 || tool.truncatedLines) && (
          <p className="text-xs text-[var(--pi-warning)]">
            Truncated: {tool.truncatedLines ?? hiddenLines} more lines
          </p>
        )}
      </ToolContent>
    </Tool>
  );
}

function ToolRunView({ tool }: { tool: ToolRun }) {
  if (tool.name === "bash") return <BashTool tool={tool} />;
  return (
    <Tool
      defaultOpen={tool.status !== "completed"}
      key={`${tool.id}-${tool.status}`}
      status={tool.status}
    >
      <ToolHeader
        state={getToolState(tool)}
        title={tool.name}
        toolName={tool.name}
        type="dynamic-tool"
      />
      <ToolContent>
        {tool.input && <ToolInput input={tool.input} />}
        <ToolOutput
          errorText={tool.status === "error" ? tool.output || "Tool execution failed" : undefined}
          output={tool.status === "error" ? undefined : tool.output}
        />
        {tool.truncatedLines ? (
          <p className="text-xs text-[var(--pi-warning)]">… ({tool.truncatedLines} more lines)</p>
        ) : null}
      </ToolContent>
    </Tool>
  );
}

function StopReason({ message }: { message: ConversationMessage }) {
  if (message.errorMessage) {
    return <p className="text-sm text-[var(--pi-error)]">Error: {message.errorMessage}</p>;
  }
  if (message.stopReason === "length") {
    return <p className="text-sm text-[var(--pi-error)]">Response was truncated before completion.</p>;
  }
  if (message.stopReason === "aborted") {
    return <p className="text-sm text-[var(--pi-error)]">Operation aborted</p>;
  }
  return null;
}

function SystemMessage({ message }: { message: ConversationMessage }) {
  if (message.kind === "compaction") {
    return (
      <details className="bg-[var(--pi-custom-bg)] px-4 py-2 text-sm text-[var(--pi-muted)]">
        <summary className="cs-smooth-interaction-fast cs-hover-brighten cs-focus-pop cs-active-scale-98 cs-tap-highlight-none cursor-pointer list-none outline-none motion-reduce:transform-none">
          <Badge className="mr-2 rounded-none bg-[#9575cd] text-white">[compaction]</Badge>
          {message.pending
            ? "Compacting context…"
            : message.errorMessage
              ? `Error: ${message.errorMessage}`
              : `Compacted from ${message.tokensBefore ?? "?"} tokens (click to expand)`}
        </summary>
        {message.content && <MessageResponse className="mt-2">{message.content}</MessageResponse>}
      </details>
    );
  }
  return (
    <div className="px-4 py-2 text-sm text-[var(--pi-muted)]">
      <Badge className="mr-2 rounded-none" variant="outline">[branch summary]</Badge>
      {message.content}
    </div>
  );
}

function App() {
  const {
    state,
    selectedDirectory,
    eventsReady,
    error,
    chooseDirectory,
    connect,
    disconnect,
    prompt,
    abort,
  } = usePiSession();
  const { connection, messages } = state;
  const [isSubmitting, setIsSubmitting] = useState(false);
  const isRunning = connection.phase === "running";
  const canSend = connection.phase === "ready";
  const canConnect =
    connection.phase === "disconnected" ||
    (connection.phase === "failed" && !connection.sessionId);
  const directory = connection.workingDirectory ?? selectedDirectory;

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const content = message.text.trim();
      if (content && canSend) {
        setIsSubmitting(true);
        try {
          await prompt(content);
        } finally {
          setIsSubmitting(false);
        }
      }
    },
    [canSend, prompt],
  );

  return (
    <TooltipProvider>
      <main className="flex h-screen min-h-0 bg-background text-foreground">
        <aside className="flex w-72 shrink-0 flex-col border-r bg-card/45 p-3">
          <div className="flex items-center gap-2 px-2 py-3">
            <div className="grid size-8 place-items-center bg-primary text-primary-foreground">
              <BotIcon className="size-4" />
            </div>
            <div>
              <h1 className="font-semibold tracking-tight">Lure</h1>
              <p className="text-xs text-muted-foreground">Pi 桌面客户端</p>
            </div>
          </div>

          <div className="mt-4 space-y-2">
            <Button
              className="w-full justify-start"
              onClick={() => void chooseDirectory()}
              variant="outline"
            >
              <FolderIcon className="size-4" />
              选择工作目录
            </Button>
            {directory && (
              <p className="break-all bg-muted px-3 py-2 text-xs text-muted-foreground">
                {directory}
              </p>
            )}
            {canConnect ? (
              <Button
                className="w-full"
                disabled={!selectedDirectory || !eventsReady}
                onClick={() => void connect()}
              >
                <LinkIcon className="size-4" />
                连接 Pi
              </Button>
            ) : (
              <Button
                className="w-full"
                onClick={() => void disconnect()}
                variant="outline"
              >
                <UnplugIcon className="size-4" />
                断开连接
              </Button>
            )}
          </div>

          <div className="mt-auto border p-3 text-xs shadow-none">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-muted-foreground">连接状态</span>
              <Badge variant="outline">{phaseLabels[connection.phase]}</Badge>
            </div>
            {connection.model && (
              <div className="space-y-1 border-t pt-2 text-muted-foreground">
                <p>
                  模型：
                  <span className="text-foreground">{connection.model.id}</span>
                </p>
                <p>Provider：{connection.model.provider}</p>
                <p>Thinking：{connection.thinkingLevel ?? "off"}</p>
              </div>
            )}
          </div>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col">
          <header className="flex min-h-14 shrink-0 items-center justify-between gap-4 border-b px-5 py-2">
            <div className="min-w-0">
              <p className="text-sm font-medium">Pi RPC 会话</p>
              <p className="truncate text-xs text-muted-foreground">
                {directory ?? "请选择工作目录并连接 Pi"}
              </p>
            </div>
            <Badge className="shrink-0 gap-1.5" variant="outline">
              <span
                className={`size-1.5 rounded-full ${
                  connection.phase === "ready"
                    ? "bg-emerald-500"
                    : connection.phase === "running"
                      ? "animate-pulse bg-blue-500"
                      : connection.phase === "failed"
                        ? "bg-destructive"
                        : "bg-amber-400"
                }`}
              />
              {phaseLabels[connection.phase]}
            </Badge>
          </header>

          {(error || state.notice) && (
            <div
              className={`mx-5 mt-3 border px-3 py-2 text-sm ${
                error
                  ? "border-destructive/40 bg-destructive/5 text-destructive"
                  : state.notice === "Compacting context…"
                    ? "border-[var(--pi-accent)]/50 text-[var(--pi-accent)]"
                    : state.notice?.startsWith("Retrying")
                      ? "border-[var(--pi-warning)]/50 text-[var(--pi-warning)]"
                      : "bg-muted text-muted-foreground"
              }`}
              role={error ? "alert" : "status"}
            >
              {error?.message ?? state.notice}
            </div>
          )}

          <Conversation className="min-h-0">
            <ConversationContent className="w-full px-4 py-4">
              {messages.length === 0 && (
                <Message from="assistant">
                  <MessageContent>
                    <MessageResponse>
                      欢迎使用 **Lure**。选择工作目录并连接 Pi 后，即可开始真实的 RPC 对话。
                    </MessageResponse>
                  </MessageContent>
                </Message>
              )}
              {messages.map((message) =>
                message.role === "system" ? (
                  <Message from="system" key={message.id}>
                    <SystemMessage message={message} />
                  </Message>
                ) : (
                  <Message from={message.role} key={message.id}>
                    <MessageContent>
                      {message.blocks.map((block) =>
                        block.type === "thinking" ? (
                          <Reasoning
                            isStreaming={isRunning && state.activeAssistantId === message.id}
                            key={`thinking-${block.contentIndex}`}
                          >
                            <ReasoningTrigger />
                            <ReasoningContent>{block.text}</ReasoningContent>
                          </Reasoning>
                        ) : (
                          <MessageResponse key={`text-${block.contentIndex}`}>
                            {block.text}
                          </MessageResponse>
                        ),
                      )}
                      {message.tools.map((tool) => (
                        <ToolRunView key={`${tool.id}-${tool.status}`} tool={tool} />
                      ))}
                      <StopReason message={message} />
                    </MessageContent>
                  </Message>
                ),
              )}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>

          <div className="shrink-0 border-t bg-background/90 px-4 py-3 backdrop-blur">
            <PromptInput
              className="w-full rounded-none shadow-none"
              onSubmit={handleSubmit}
            >
              <PromptInputBody>
                <PromptInputTextarea
                  disabled={!canSend}
                  placeholder={
                    canSend
                      ? "给 Pi 发送消息…"
                      : isRunning
                        ? "Pi 正在运行…"
                        : "连接 Pi 后即可发送消息…"
                  }
                />
              </PromptInputBody>
              <PromptInputFooter>
                <PromptInputTools>
                  <span className="px-2 text-xs text-muted-foreground">
                    {isRunning
                      ? "Pi 正在运行，可点击停止"
                      : "Enter 发送 · Shift+Enter 换行"}
                  </span>
                </PromptInputTools>
                <PromptInputSubmit
                  aria-label={isRunning ? "停止生成" : "发送消息"}
                  disabled={!canSend && !isRunning}
                  onStop={() => void abort()}
                  status={error ? "error" : isRunning ? "streaming" : isSubmitting ? "submitted" : "ready"}
                />
              </PromptInputFooter>
            </PromptInput>
          </div>
        </section>
      </main>
    </TooltipProvider>
  );
}

export default App;
