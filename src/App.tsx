import {
  ThreadPrimitive,
  useAui,
  type AppendMessage,
} from "@assistant-ui/react";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { selectImageFiles, readImageAttachments } from "@/features/pi-connection/api";
import { usePiSession } from "@/features/pi-connection/use-pi-session";
import {
  PiAssistantRuntimeProvider,
  readAppendMessageImages,
  readAppendMessageText,
} from "@/features/workspace/assistant-runtime";
import { ConnectionLoading } from "@/features/workspace/connection-loading";
import { ConversationStream } from "@/features/workspace/conversation-stream";
import { EmptyState } from "@/features/workspace/empty-state";
import { ErrorPanel } from "@/features/workspace/error-panel";
import { ExtensionUiDialog } from "@/features/workspace/extension-ui-dialog";
import { readDraft, readTitle, writeDraft, writeTitle } from "@/features/workspace/local-preferences";
import { serializeConversation } from "@/features/workspace/presentation";
import { PromptCard } from "@/features/workspace/prompt-card";
import { RiskConfirmDialog } from "@/features/workspace/risk-confirm-dialog";
import { TaskHeader } from "@/features/workspace/task-header";
import { TaskNavigation } from "@/features/workspace/task-navigation";
import { ArrowDownIcon, MenuIcon } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

function directoryNameOf(directory: string | null) {
  if (!directory) return null;
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

function ComposerDraftInitializer({ initialText }: { initialText: string }) {
  const aui = useAui();
  const restored = useRef(false);

  useLayoutEffect(() => {
    if (restored.current) return;
    restored.current = true;
    aui.composer.setText(initialText);
  }, [aui, initialText]);

  return null;
}

function ComposerEmptyState({
  projectName,
  onDraftChange,
}: {
  projectName: string | null;
  onDraftChange: (value: string) => void;
}) {
  const aui = useAui();
  return (
    <EmptyState
      onPickExample={(value) => {
        aui.composer.setText(value);
        onDraftChange(value);
      }}
      projectName={projectName}
    />
  );
}

function App() {
  const {
    state,
    selectedDirectory,
    eventsReady,
    error,
    availableModels,
    workspaceContext,
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
  const [composerGeneration, setComposerGeneration] = useState(0);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [pendingRisk, setPendingRisk] = useState<"export" | null>(null);
  const promptInputRef = useRef<HTMLTextAreaElement>(null);

  const directory = connection.workingDirectory ?? selectedDirectory;
  const directoryName = directoryNameOf(directory);
  const branch = workspaceContext?.branch ?? null;

  // 草稿与标题按工作目录保存在本机，切换任务或意外关闭后可以恢复。
  const persistDraft = useCallback(
    (value: string) => writeDraft(selectedDirectory, value),
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

  const handleRetry = useCallback(async () => {
    if (!canConnectSession) await disconnect();
    if (selectedDirectory) await connect();
    else await newConversation();
  }, [canConnectSession, connect, disconnect, newConversation, selectedDirectory]);

  const handleAssistantNew = useCallback(
    async (message: AppendMessage) => {
      const text = readAppendMessageText(message);
      const images = readAppendMessageImages(message);
      if ((!text && images.length === 0) || !canSend) return;
      setIsSubmitting(true);
      try {
        await prompt(text, images);
        persistDraft("");
      } finally {
        setIsSubmitting(false);
      }
    },
    [canSend, persistDraft, prompt],
  );

  const pickImages = useCallback(async () => {
    try {
      const paths = await selectImageFiles();
      if (paths.length === 0) return [];
      return await readImageAttachments(paths);
    } catch {
      return [];
    }
  }, []);

  const handleNewTask = useCallback(async () => {
    const created = await newConversation();
    if (!created) return;
    persistDraft("");
    setComposerGeneration((value) => value + 1);
    setTaskTitle(directoryName ?? "新任务");
    setNavigationOpen(false);
  }, [directoryName, newConversation, persistDraft, setTaskTitle]);

  useEffect(() => {
    if (composerGeneration > 0) promptInputRef.current?.focus();
  }, [composerGeneration]);

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
        canSend={canSend}
        isRunning={isRunning}
        key={`${selectedDirectory ?? "pending"}:${composerGeneration}`}
        messages={messages}
        onCancel={abort}
        onNew={handleAssistantNew}
      >
        <ComposerDraftInitializer initialText={readDraft(selectedDirectory)} />
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
              <div className="mx-auto flex min-h-full w-full max-w-[920px] flex-col gap-4 px-4 py-6 md:px-6">
                {connection.phase === "connecting" ? (
                  <ConnectionLoading />
                ) : hasConversation ? (
                  <ConversationStream />
                ) : (
                  <ComposerEmptyState
                    onDraftChange={persistDraft}
                    projectName={directoryName}
                  />
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

              <PromptCard
                branch={branch}
                canConnectSession={canConnectSession}
                directoryName={directoryName}
                eventsReady={eventsReady}
                isRunning={isRunning}
                model={connection.model}
                models={availableModels}
                onAddImages={pickImages}
                onConnect={() => void connect()}
                onDraftChange={persistDraft}
                onSelectModel={(provider, modelId) => void setModel(provider, modelId)}
                onSelectThinkingLevel={(level) => void setThinkingLevel(level)}
                onStop={() => void abort()}
                phase={connection.phase}
                textareaRef={promptInputRef}
                thinkingLevel={connection.thinkingLevel}
              />
            </ThreadPrimitive.Viewport>
          </ThreadPrimitive.Root>

          {isSubmitting && <span className="sr-only">正在提交指令</span>}
        </main>

        <ExtensionUiDialog onRespond={respondToExtension} request={extensionRequest} />

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