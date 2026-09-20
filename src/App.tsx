import { ThreadPrimitive, type AppendMessage } from "@assistant-ui/react";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { selectImageFiles, readImageAttachments } from "@/features/pi-connection/api";
import { usePiSession } from "@/features/pi-connection/use-pi-session";
import {
  PiAssistantRuntimeProvider,
  readAppendMessageText,
} from "@/features/workspace/assistant-runtime";
import { ConversationStream } from "@/features/workspace/conversation-stream";
import { EmptyState } from "@/features/workspace/empty-state";
import { ErrorPanel } from "@/features/workspace/error-panel";
import { ExtensionUiDialog } from "@/features/workspace/extension-ui-dialog";
import { readDraft, readTitle, writeDraft, writeTitle } from "@/features/workspace/local-preferences";
import { serializeConversation } from "@/features/workspace/presentation";
import { PromptCard, type PromptAttachment } from "@/features/workspace/prompt-card";
import { RiskConfirmDialog } from "@/features/workspace/risk-confirm-dialog";
import { TaskHeader } from "@/features/workspace/task-header";
import { TaskNavigation } from "@/features/workspace/task-navigation";
import { ArrowDownIcon, MenuIcon } from "lucide-react";
import { useCallback, useRef, useState } from "react";

function directoryNameOf(directory: string | null) {
  if (!directory) return null;
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

function App() {
  const {
    state,
    selectedDirectory,
    eventsReady,
    error,
    availableModels,
    workspaceContext,
    chooseDirectory,
    connect,
    newConversation,
    disconnect,
    prompt,
    abort,
    setModel,
    setThinkingLevel,
    respondToExtension,
  } = usePiSession();
  const { connection, messages, extensionRequest } = state;
  const [navigationOpen, setNavigationOpen] = useState(false);
  const [taskQuery, setTaskQuery] = useState("");
  const [attachments, setAttachments] = useState<PromptAttachment[]>([]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [pendingRisk, setPendingRisk] = useState<"images" | "export" | null>(null);
  const promptInputRef = useRef<HTMLTextAreaElement>(null);

  const directory = connection.workingDirectory ?? selectedDirectory;
  const directoryName = directoryNameOf(directory);
  const branch = workspaceContext?.branch ?? null;

  // 草稿与标题按工作目录保存在本机，切换任务或意外关闭后可以恢复。
  const [draftState, setDraftState] = useState(() => ({
    scope: selectedDirectory,
    value: readDraft(selectedDirectory),
  }));
  if (draftState.scope !== selectedDirectory) {
    setDraftState({ scope: selectedDirectory, value: readDraft(selectedDirectory) });
  }
  const draft = draftState.value;
  const setDraft = useCallback(
    (value: string) => {
      writeDraft(selectedDirectory, value);
      setDraftState({ scope: selectedDirectory, value });
    },
    [selectedDirectory],
  );

  const [titleState, setTitleState] = useState(() => ({
    scope: selectedDirectory,
    value: readTitle(selectedDirectory),
  }));
  if (titleState.scope !== selectedDirectory) {
    setTitleState({ scope: selectedDirectory, value: readTitle(selectedDirectory) });
  }
  const taskTitle = titleState.value ?? directoryName ?? "新任务";
  const setTaskTitle = useCallback(
    (value: string) => {
      writeTitle(selectedDirectory, value);
      setTitleState({ scope: selectedDirectory, value });
    },
    [selectedDirectory],
  );
  const hasConversation = messages.length > 0;
  const isRunning = connection.phase === "running";
  const canSend = connection.phase === "ready";
  const canConnectSession =
    connection.phase === "disconnected" ||
    (connection.phase === "failed" && !connection.sessionId);
  const canSubmit = canSend && (draft.trim().length > 0 || attachments.length > 0);

  const handleRetry = useCallback(async () => {
    if (!canConnectSession) await disconnect();
    if (selectedDirectory) await connect();
    else await newConversation();
  }, [canConnectSession, connect, disconnect, newConversation, selectedDirectory]);

  const submitPrompt = useCallback(
    async (content: string) => {
      const text = content.trim();
      if (!text || !canSend) return;
      setIsSubmitting(true);
      try {
        await prompt(
          text,
          attachments.map(({ data, mimeType }) => ({ data, mimeType })),
        );
        setDraft("");
        setAttachments([]);
      } finally {
        setIsSubmitting(false);
      }
    },
    [attachments, canSend, prompt, setDraft],
  );

  const handleAssistantNew = useCallback(
    async (message: AppendMessage) => {
      await submitPrompt(readAppendMessageText(message));
    },
    [submitPrompt],
  );

  const pickImages = useCallback(async () => {
    try {
      const paths = await selectImageFiles();
      if (paths.length === 0) return;
      const images = await readImageAttachments(paths);
      setAttachments((previous) => [...previous, ...images]);
    } catch {
      // 选择或读取失败时保持当前草稿不变。
    }
  }, []);

  const confirmAddImages = useCallback(async () => {
    setPendingRisk(null);
    await pickImages();
  }, [pickImages]);

  const handleNewTask = useCallback(async () => {
    const created = await newConversation();
    if (!created) return;
    setDraft("");
    setTaskTitle(directoryName ?? "新任务");
    setAttachments([]);
    setNavigationOpen(false);
    promptInputRef.current?.focus();
  }, [directoryName, newConversation, setDraft, setTaskTitle]);

  const confirmExport = useCallback(() => {
    setPendingRisk(null);
    const markdown = serializeConversation(taskTitle, messages);
    const blob = new Blob([markdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.download = `${taskTitle}.md`;
    link.href = url;
    document.body.append(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }, [messages, taskTitle]);

  return (
    <TooltipProvider>
      <PiAssistantRuntimeProvider
        activeAssistantId={state.activeAssistantId}
        isRunning={isRunning}
        isSendDisabled={!canSend}
        messages={messages}
        onCancel={abort}
        onNew={handleAssistantNew}
      >
        <div className="relative flex h-dvh min-h-0 overflow-hidden bg-background text-foreground">
        {navigationOpen && (
          <button
            aria-label="关闭导航遮罩"
            className="fixed inset-0 z-30 bg-black/20 md:hidden"
            onClick={() => setNavigationOpen(false)}
            type="button"
          />
        )}

        <TaskNavigation
          hasTask={Boolean(directory)}
          onClose={() => setNavigationOpen(false)}
          onNewTask={() => void handleNewTask()}
          onQueryChange={setTaskQuery}
          onSelectTask={() => setNavigationOpen(false)}
          open={navigationOpen}
          phase={connection.phase}
          query={taskQuery}
          taskTitle={taskTitle}
        />

        <main aria-label="任务工作区" className="flex min-w-0 flex-1 flex-col">
          {hasConversation ? (
            <TaskHeader
              canDisconnect={!canConnectSession}
              onChooseDirectory={() => void chooseDirectory()}
              onDisconnect={() => void disconnect()}
              onExport={() => setPendingRisk("export")}
              onOpenNavigation={() => setNavigationOpen(true)}
              onTitleChange={setTaskTitle}
              title={taskTitle}
            />
          ) : (
            <div className="px-4 pt-4 md:px-5">
              <Button
                aria-expanded={navigationOpen}
                aria-label="打开任务导航"
                className="md:hidden"
                onClick={() => setNavigationOpen(true)}
                size="icon"
                type="button"
                variant="ghost"
              >
                <MenuIcon />
              </Button>
            </div>
          )}

          {error && (
            <div className="px-4 pt-3 md:px-6">
              <ErrorPanel
                canRetry={connection.phase === "failed"}
                error={error}
                onRetry={() => void handleRetry()}
              />
            </div>
          )}

          <ThreadPrimitive.Root
            aria-label="对话线程"
            className="relative mx-auto flex min-h-0 w-full max-w-[920px] flex-1 flex-col"
            role="log"
          >
            <ThreadPrimitive.Viewport className="min-h-0 flex-1 overflow-y-auto">
              <div className="flex min-h-full flex-col gap-4 px-4 py-6 md:px-6">
                {hasConversation ? (
                  <ConversationStream messages={messages} />
                ) : (
                  <EmptyState onPickExample={setDraft} projectName={directoryName} />
                )}
              </div>
              <ThreadPrimitive.ScrollToBottom asChild>
                <Button
                  aria-label="滚动到最新消息"
                  className="sticky bottom-4 left-1/2 -translate-x-1/2 rounded-full shadow-sm disabled:hidden"
                  size="icon"
                  type="button"
                  variant="outline"
                >
                  <ArrowDownIcon className="size-4" />
                </Button>
              </ThreadPrimitive.ScrollToBottom>
            </ThreadPrimitive.Viewport>
          </ThreadPrimitive.Root>

          <PromptCard
            attachments={attachments}
            branch={branch}
            canConnectSession={canConnectSession}
            canSubmit={canSubmit}
            directoryName={directoryName}
            draft={draft}
            eventsReady={eventsReady}
            isRunning={isRunning}
            model={connection.model}
            models={availableModels}
            onAddImages={() => setPendingRisk("images")}
            onChooseDirectory={() => void chooseDirectory()}
            onConnect={() => void connect()}
            onDraftChange={setDraft}
            onRemoveAttachment={(name) =>
              setAttachments((previous) => previous.filter((item) => item.name !== name))
            }
            onSelectModel={(provider, modelId) => void setModel(provider, modelId)}
            onSelectThinkingLevel={(level) => void setThinkingLevel(level)}
            onStop={() => void abort()}
            phase={connection.phase}
            textareaRef={promptInputRef}
            thinkingLevel={connection.thinkingLevel}
          />

          {isSubmitting && <span className="sr-only">正在提交指令</span>}
        </main>

        <ExtensionUiDialog onRespond={respondToExtension} request={extensionRequest} />

        <RiskConfirmDialog
          confirmLabel="继续选择图片"
          details={{
            action: "把所选图片作为附件随指令发送给 Pi。",
            recoverable: "本地文件不会被修改；图片内容会随指令发送给模型服务，发出后无法撤回。",
            target: "你选择的本地图片文件。",
          }}
          onCancel={() => setPendingRisk(null)}
          onConfirm={() => void confirmAddImages()}
          open={pendingRisk === "images"}
          title="发送图片前确认"
        />

        <RiskConfirmDialog
          confirmLabel="导出并保存"
          details={{
            action: "把当前任务的对话导出为 Markdown 文件并保存到本机。",
            recoverable: "导出文件保存在你指定的位置，桌面端无法自动清除。",
            target: "当前任务的全部对话内容及其中出现的本地路径。",
          }}
          onCancel={() => setPendingRisk(null)}
          onConfirm={confirmExport}
          open={pendingRisk === "export"}
          title="导出任务记录前确认"
        />
        </div>
      </PiAssistantRuntimeProvider>
    </TooltipProvider>
  );
}

export default App;