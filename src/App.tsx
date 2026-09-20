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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { TooltipProvider } from "@/components/ui/tooltip";
import { usePiSession } from "@/features/pi-connection/use-pi-session";
import {
  BotIcon,
  ChevronRightIcon,
  FolderIcon,
  LinkIcon,
  LoaderCircleIcon,
  MenuIcon,
  MoreHorizontalIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  UnplugIcon,
  XIcon,
} from "lucide-react";
import { useCallback, useState } from "react";
import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";

const phaseLabels = {
  disconnected: "准备中",
  connecting: "准备中",
  ready: "准备中",
  running: "执行中",
  failed: "已失败",
} as const;

const workflowExamples = ["分析当前项目", "检查未提交改动", "解释代码结构"] as const;

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
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [taskQuery, setTaskQuery] = useState("");
  const [draft, setDraft] = useState("");
  const isRunning = connection.phase === "running";
  const canSend = connection.phase === "ready";
  const canConnect =
    connection.phase === "disconnected" ||
    (connection.phase === "failed" && !connection.sessionId);
  const directory = connection.workingDirectory ?? selectedDirectory;
  const taskTitle = directory
    ? directory.split(/[\\/]/).filter(Boolean).at(-1) ?? "当前任务"
    : "新任务";
  const taskMatchesSearch = taskTitle
    .toLocaleLowerCase()
    .includes(taskQuery.trim().toLocaleLowerCase());

  const handleSubmit = useCallback(
    async (message: PromptInputMessage) => {
      const content = message.text.trim();
      if (content && canSend) {
        setIsSubmitting(true);
        try {
          await prompt(content);
          setDraft("");
        } finally {
          setIsSubmitting(false);
        }
      }
    },
    [canSend, prompt],
  );

  const handleNewTask = useCallback(async () => {
    if (!canConnect) await disconnect();
    await chooseDirectory();
    setNavigationOpen(false);
  }, [canConnect, chooseDirectory, disconnect]);

  return (
    <TooltipProvider>
      <div className="relative flex h-dvh min-h-0 overflow-hidden bg-background text-foreground">
        {navigationOpen && (
          <button
            aria-label="关闭导航遮罩"
            className="fixed inset-0 z-30 bg-black/20 md:hidden"
            onClick={() => setNavigationOpen(false)}
            type="button"
          />
        )}

        <nav
          aria-label="任务导航"
          className={`fixed inset-y-0 left-0 z-40 flex w-64 shrink-0 flex-col border-r bg-card p-3 transition-transform duration-200 md:static md:translate-x-0 ${
            navigationOpen ? "translate-x-0" : "-translate-x-full"
          }`}
        >
          <div className="flex min-h-12 items-center gap-2 px-2">
            <div className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground">
              <BotIcon className="size-4" />
            </div>
            <div className="min-w-0">
              <p className="font-semibold tracking-tight">Lure</p>
              <p className="text-xs text-muted-foreground">个人智能体工作台</p>
            </div>
            <Button
              aria-expanded={navigationOpen}
              aria-label="关闭任务导航"
              className="ml-auto md:hidden"
              onClick={() => setNavigationOpen(false)}
              size="icon"
              variant="ghost"
            >
              <XIcon />
            </Button>
          </div>

          <Button className="mt-4 w-full justify-start" onClick={() => void handleNewTask()}>
            <PlusIcon />
            新建任务
          </Button>

          <label className="relative mt-3 block">
            <SearchIcon className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <input
              aria-label="搜索任务"
              className="h-9 w-full rounded-lg border bg-background pl-9 pr-3 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
              onChange={(event) => setTaskQuery(event.target.value)}
              placeholder="搜索任务"
              type="search"
              value={taskQuery}
            />
          </label>

          <div className="mt-5 min-h-0 flex-1 overflow-y-auto px-1">
            <section aria-labelledby="today-tasks">
              <h2 className="px-2 text-xs font-medium text-muted-foreground" id="today-tasks">
                今天
              </h2>
              {directory && taskMatchesSearch ? (
                <button
                  aria-current="page"
                  className="mt-1 flex w-full items-center gap-2 rounded-lg bg-accent px-2.5 py-2 text-left text-sm font-medium text-accent-foreground"
                  onClick={() => setNavigationOpen(false)}
                  type="button"
                >
                  <span className="size-2 shrink-0 rounded-full bg-[var(--pi-accent)]" />
                  <span className="min-w-0 flex-1 truncate">{taskTitle}</span>
                  <span className="text-[11px] font-normal text-muted-foreground">
                    {phaseLabels[connection.phase]}
                  </span>
                </button>
              ) : (
                <p className="px-2 py-2 text-xs text-muted-foreground">
                  {taskQuery ? "没有匹配的任务" : "暂无任务"}
                </p>
              )}
            </section>

            <section aria-labelledby="recent-tasks" className="mt-5">
              <h2 className="px-2 text-xs font-medium text-muted-foreground" id="recent-tasks">
                最近
              </h2>
              <p className="px-2 py-2 text-xs text-muted-foreground">暂无最近任务</p>
            </section>

            <details className="group mt-3">
              <summary className="flex cursor-pointer list-none items-center gap-1 rounded-lg px-2 py-2 text-xs font-medium text-muted-foreground hover:bg-accent">
                <ChevronRightIcon className="size-3.5 transition-transform group-open:rotate-90" />
                <span>已归档</span>
              </summary>
              <p className="px-7 py-1 text-xs text-muted-foreground">暂无已归档任务</p>
            </details>
          </div>

          <Button className="mt-3 w-full justify-start" variant="ghost">
            <SettingsIcon />
            设置
          </Button>
        </nav>

        <main aria-label="任务工作区" className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-14 shrink-0 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur md:px-5">
            <Button
              aria-expanded={navigationOpen}
              aria-label="打开任务导航"
              className="md:hidden"
              onClick={() => setNavigationOpen(true)}
              size="icon"
              variant="ghost"
            >
              <MenuIcon />
            </Button>
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-sm font-semibold">{taskTitle}</h1>
              <p className="truncate text-xs text-muted-foreground">
                {directory ?? "选择工作目录后开始任务"}
              </p>
            </div>
            {connection.model && (
              <span className="hidden max-w-48 truncate text-xs text-muted-foreground sm:block">
                <span>{connection.model.id}</span>
                <span> · {connection.thinkingLevel ?? "off"}</span>
              </span>
            )}
            <Badge
              aria-live="polite"
              className="shrink-0 gap-1.5"
              role="status"
              variant="outline"
            >
              <span
                aria-hidden="true"
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
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button aria-label="更多任务操作" size="icon" variant="ghost">
                  <MoreHorizontalIcon />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onSelect={() => void chooseDirectory()}>
                  <FolderIcon />
                  选择工作目录
                </DropdownMenuItem>
                {!canConnect && (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem onSelect={() => void disconnect()}>
                      <UnplugIcon />
                      断开 Pi
                    </DropdownMenuItem>
                  </>
                )}
              </DropdownMenuContent>
            </DropdownMenu>
          </header>

          {(error || state.notice) && (
            <div
              className={`mx-auto mt-3 w-[calc(100%-2rem)] max-w-[1080px] rounded-lg border px-3 py-2 text-sm ${
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
            <ConversationContent className="mx-auto w-full max-w-[1080px] gap-4 px-4 py-6 md:px-6">
              {messages.length === 0 && (
                <div className="mx-auto flex w-full max-w-2xl flex-col items-center py-10 text-center md:py-16">
                  <div className="mb-5 grid size-12 place-items-center rounded-xl bg-accent text-accent-foreground">
                    <BotIcon className="size-6" />
                  </div>
                  <h2 className="text-xl font-semibold tracking-tight">开始一个新任务</h2>
                  <p className="mt-2 max-w-lg text-sm leading-6 text-muted-foreground">
                    选择项目目录并连接 Pi，然后描述你想完成的工作。
                  </p>

                  <div className="mt-6 flex flex-wrap justify-center gap-2">
                    {workflowExamples.map((example) => (
                      <Button
                        key={example}
                        onClick={() => setDraft(example)}
                        size="sm"
                        variant="outline"
                      >
                        {example}
                      </Button>
                    ))}
                  </div>

                  <div className="mt-7 flex flex-col items-center gap-3 rounded-xl border bg-card p-4 text-left sm:flex-row">
                    <div className="min-w-0 flex-1">
                      <p className="text-sm font-medium">
                        {directory ? taskTitle : "尚未选择工作目录"}
                      </p>
                      <p className="mt-1 break-all text-xs text-muted-foreground">
                        {directory ?? "工作目录用于启动当前 Pi 会话"}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-2">
                      <Button onClick={() => void chooseDirectory()} variant="outline">
                        <FolderIcon />
                        选择工作目录
                      </Button>
                      {canConnect && (
                        <Button
                          disabled={!selectedDirectory || !eventsReady}
                          onClick={() => void connect()}
                        >
                          <LinkIcon />
                          连接 Pi
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
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

          <div className="shrink-0 border-t bg-background/95 px-4 py-3 backdrop-blur md:px-6">
            <div className="mx-auto w-full max-w-[1080px]">
              <PromptInput className="w-full rounded-xl shadow-none" onSubmit={handleSubmit}>
                <PromptInputBody>
                  <PromptInputTextarea
                    disabled={!canSend}
                    onChange={(event) => setDraft(event.target.value)}
                    placeholder={
                      canSend
                        ? "给 Pi 发送消息…"
                        : isRunning
                          ? "Pi 正在运行…"
                          : "连接 Pi 后即可发送消息…"
                    }
                    value={draft}
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
                    status={
                      error
                        ? "error"
                        : isRunning
                          ? "streaming"
                          : isSubmitting
                            ? "submitted"
                            : "ready"
                    }
                  />
                </PromptInputFooter>
              </PromptInput>
            </div>
          </div>
        </main>
      </div>
    </TooltipProvider>
  );
}

export default App;
