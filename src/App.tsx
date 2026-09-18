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
import { Tool, ToolHeader } from "@/components/ai-elements/tool";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { usePiSession } from "@/features/pi-connection/use-pi-session";
import {
  BotIcon,
  FolderIcon,
  LinkIcon,
  UnplugIcon,
} from "lucide-react";
import { useCallback } from "react";

const phaseLabels = {
  disconnected: "未连接",
  connecting: "正在连接",
  ready: "已连接",
  running: "运行中",
  failed: "连接异常",
} as const;

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
  const isRunning = connection.phase === "running";
  const canSend = connection.phase === "ready";
  const canConnect =
    connection.phase === "disconnected" ||
    (connection.phase === "failed" && !connection.sessionId);
  const directory = connection.workingDirectory ?? selectedDirectory;

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const content = message.text.trim();
      if (content && canSend) await prompt(content);
    },
    [canSend, prompt],
  );

  return (
    <TooltipProvider>
      <main className="flex h-screen min-h-0 bg-background text-foreground">
        <aside className="flex w-72 shrink-0 flex-col border-r bg-card/45 p-3">
          <div className="flex items-center gap-2 px-2 py-3">
            <div className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground">
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
              <p className="break-all rounded-md bg-muted px-3 py-2 text-xs text-muted-foreground">
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

          <div className="mt-auto rounded-lg border p-3 text-xs">
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
              className={`mx-5 mt-3 rounded-md border px-3 py-2 text-sm ${
                error
                  ? "border-destructive/40 bg-destructive/5 text-destructive"
                  : "bg-muted text-muted-foreground"
              }`}
              role={error ? "alert" : "status"}
            >
              {error?.message ?? state.notice}
            </div>
          )}

          <Conversation className="min-h-0">
            <ConversationContent className="mx-auto w-full max-w-3xl px-6 py-8">
              {messages.length === 0 && (
                <Message from="assistant">
                  <MessageContent>
                    <MessageResponse>
                      欢迎使用 **Lure**。选择工作目录并连接 Pi 后，即可开始真实的 RPC 对话。
                    </MessageResponse>
                  </MessageContent>
                </Message>
              )}
              {messages.map((message) => (
                <Message from={message.role} key={message.id}>
                  <MessageContent>
                    {message.thinking && (
                      <Reasoning
                        isStreaming={
                          isRunning && state.activeAssistantId === message.id
                        }
                      >
                        <ReasoningTrigger>查看思考过程</ReasoningTrigger>
                        <ReasoningContent>{message.thinking}</ReasoningContent>
                      </Reasoning>
                    )}
                    {message.content && (
                      <MessageResponse>{message.content}</MessageResponse>
                    )}
                    {message.tools.map((tool) => (
                      <Tool key={tool.id}>
                        <ToolHeader
                          state={
                            tool.status === "running"
                              ? "input-available"
                              : tool.status === "error"
                                ? "output-error"
                                : "output-available"
                          }
                          title={tool.name}
                          toolName={tool.name}
                          type="dynamic-tool"
                        />
                      </Tool>
                    ))}
                  </MessageContent>
                </Message>
              ))}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>

          <div className="shrink-0 border-t bg-background/90 px-6 py-4 backdrop-blur">
            <PromptInput
              className="mx-auto w-full max-w-3xl rounded-2xl"
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
                  status={isRunning ? "streaming" : "ready"}
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
