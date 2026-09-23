import {
  ThreadPrimitive,
  useAui,
  type AppendMessage,
} from "@assistant-ui/react";
import { useAppDispatch } from "@/app/hooks";
import { Button } from "@/components/ui/button";
import {
  readImageAttachments,
  selectImageFiles,
} from "@/lib/pi-rpc";
import {
  ConversationStream,
  EmptyState,
  PiAssistantRuntimeProvider,
  PromptCard,
  readAppendMessageImages,
  readAppendMessageText,
  readDraft,
  useConversation,
  writeDraft,
} from "@/features/conversation";
import { ExtensionUiDialog, useExtensionUi } from "@/features/extension-ui";
import { useModels } from "@/features/models";
import {
  ConnectionFailureDialog,
  CreateProjectDialog,
  ErrorPanel,
  readTitle,
  sessionsActions,
  TaskNavigation,
  useSessions,
  writeTitle,
} from "@/features/sessions";
import { WindowTitleBar } from "@/app/window-title-bar";
import { ArrowDownIcon } from "lucide-react";
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

export function AppShell() {
  const dispatch = useAppDispatch();
  useEffect(() => {
    dispatch(sessionsActions.eventSubscriptionRequested());
    return () => { dispatch(sessionsActions.eventSubscriptionReleased()); };
  }, [dispatch]);
  const {
    selectedDirectory,
    error,
    connectionError,
    workspaceContext,
    connection,
    retry,
    newDefaultConversation,
    newProjectConversation,
    addProject,
    defaultWorkspace,
    projects,
    recentSessions,
    projectSessions,
    recentSessionsHasMore,
    projectSessionsHasMore,
    loadMoreSessions,
    expandedProjects,
    loadingDirectories,
    sessionTransition,
    activeSessionSummary,
    toggleProject,
    openConversation,
    chooseProjectDirectory,
    clearProjectDirectory,
    projectDirectoryCandidate,
  } = useSessions();
  const { messages, activeAssistantId, promptSubmissionCount, prompt, abort } = useConversation();
  const { availableModels, current: currentModel, thinkingLevel, setModel, setThinkingLevel } = useModels();
  const { request: extensionRequest, respond: respondToExtension } = useExtensionUi();
  const submittedPrompts = useRef(promptSubmissionCount);
  const [composerGeneration, setComposerGeneration] = useState(0);
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [dismissedConnectionError, setDismissedConnectionError] = useState<string | null>(null);
  const promptInputRef = useRef<HTMLTextAreaElement>(null);

  const directory = connection.workingDirectory ?? selectedDirectory;
  const directoryName = directoryNameOf(directory);
  const branch = workspaceContext?.branch ?? null;
  // “当前会话”锚定在事务事实（activeSessionSummary）上，而不是连接快照：
  // 连接快照可能被迟到的事件短暂覆盖，用它判定选中态会让列表闪现兑底条目。
  const activeSessionId = activeSessionSummary?.id ?? connection.sessionId;

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
  // 会话切换事务未结束时不能发送：目标会话的内容尚未确定。
  const canSend = connection.phase === "ready" && !sessionTransition;
  const switchingSession = Boolean(sessionTransition);
  // 连接失败只在发生时不请自来地提醒一次，关闭后不反复打扰。
  const connectionErrorKey = connectionError
    ? `${connectionError.code}:${connectionError.message}`
    : null;
  const connectionFailureOpen =
    connectionErrorKey !== null && connectionErrorKey !== dismissedConnectionError;
  const clearCommandError = useCallback(() => {
    dispatch(sessionsActions.commandErrorCleared());
  }, [dispatch]);

  const handleAssistantNew = useCallback(
    async (message: AppendMessage) => {
      const text = readAppendMessageText(message);
      const images = readAppendMessageImages(message);
      if ((!text && images.length === 0) || !canSend) return;
      prompt(text, images);
    },
    [canSend, prompt],
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
    const target = defaultWorkspace ?? selectedDirectory;
    newDefaultConversation();
    if (target) {
      writeDraft(target, "");
      const nextTitle = directoryNameOf(target) ?? "新任务";
      writeTitle(target, nextTitle);
      setTitleState({ scope: target, value: nextTitle });
    }
    setComposerGeneration((value) => value + 1);
  }, [defaultWorkspace, newDefaultConversation, selectedDirectory]);

  const handleNewProjectTask = useCallback(
    (targetDirectory: string) => {
      newProjectConversation(targetDirectory);
      writeDraft(targetDirectory, "");
      const nextTitle = directoryNameOf(targetDirectory) ?? "新任务";
      writeTitle(targetDirectory, nextTitle);
      setTitleState({ scope: targetDirectory, value: nextTitle });
      setComposerGeneration((value) => value + 1);
    },
    [newProjectConversation],
  );

  const handleCreateProject = useCallback(
    (name: string, targetDirectory: string) => {
      addProject(name, targetDirectory);
      handleNewProjectTask(targetDirectory);
    },
    [addProject, handleNewProjectTask],
  );

  useEffect(() => {
    if (composerGeneration > 0) promptInputRef.current?.focus();
  }, [composerGeneration]);

  useEffect(() => {
    if (promptSubmissionCount === submittedPrompts.current) return;
    submittedPrompts.current = promptSubmissionCount;
    persistDraft("");
  }, [persistDraft, promptSubmissionCount]);

  return (
      <PiAssistantRuntimeProvider
        activeAssistantId={activeAssistantId}
        canSend={canSend}
        isRunning={isRunning}
        key={`${selectedDirectory ?? "pending"}:${composerGeneration}`}
        messages={messages}
        onCancel={abort}
        onNew={handleAssistantNew}
      >
        <ComposerDraftInitializer initialText={readDraft(selectedDirectory)} />
        <div
          className="flex h-dvh min-h-0 flex-col overflow-hidden rounded-2xl border border-border bg-background text-foreground"
          data-testid="app-frame"
        >
          <WindowTitleBar
            collapsed={sidebarCollapsed}
            hasConversation={hasConversation}
            onTitleChange={setTaskTitle}
            onToggleSidebar={() => setSidebarCollapsed((collapsed) => !collapsed)}
            title={taskTitle}
          />
          <div className="relative flex min-h-0 flex-1 overflow-hidden overscroll-none">
            {!sidebarCollapsed && (
              <TaskNavigation
                activeDirectory={directory}
                activeSessionId={activeSessionId}
                canOpenConversation={canSend}
                defaultWorkspace={defaultWorkspace}
                disabled={connection.phase === "running" || connection.phase === "connecting" || switchingSession}
                expandedProjects={expandedProjects}
                hasTask={Boolean(directory)}
                loadingDirectories={loadingDirectories}
                onNewTask={() => void handleNewTask()}
                onNewProject={() => setCreateProjectOpen(true)}
                onNewProjectTask={handleNewProjectTask}
                onOpenConversation={openConversation}
                onToggleProject={toggleProject}
                onLoadMoreSessions={loadMoreSessions}
                phase={connection.phase}
                projects={projects}
                projectSessions={projectSessions}
                projectSessionsHasMore={projectSessionsHasMore}
                recentSessions={recentSessions}
                recentSessionsHasMore={recentSessionsHasMore}
                sessionTransition={sessionTransition}
                taskTitle={taskTitle}
              />
            )}

            <main aria-label="任务工作区" className="flex min-w-0 flex-1 flex-col">
          {error && (
            <div className="px-4 pt-3 md:px-6">
              <ErrorPanel error={error} onDismiss={clearCommandError} />
            </div>
          )}

          <ThreadPrimitive.Root
            aria-label="对话线程"
            className="relative flex min-h-0 w-full flex-1 flex-col"
            role="log"
          >
            <ThreadPrimitive.Viewport
              aria-label="对话滚动区"
              className="flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto overscroll-none"
            >
              <div
                aria-label="对话内容列"
                className="mx-auto flex w-full max-w-[768px] flex-1 flex-col gap-4 px-4 py-6 md:px-6"
              >
                {hasConversation ? (
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
                directoryName={directoryName}
                isRunning={isRunning}
                isSwitchingSession={switchingSession}
                model={currentModel}
                models={availableModels}
                onAddImages={pickImages}
                onDraftChange={persistDraft}
                onSelectModel={(provider, modelId) => void setModel(provider, modelId)}
                onSelectThinkingLevel={(level) => void setThinkingLevel(level)}
                onStop={() => void abort()}
                phase={connection.phase}
                textareaRef={promptInputRef}
                thinkingLevel={thinkingLevel}
              />
            </ThreadPrimitive.Viewport>
          </ThreadPrimitive.Root>

            </main>

        <ExtensionUiDialog onRespond={(value, cancelled) => { respondToExtension(value, cancelled); return Promise.resolve(); }} request={extensionRequest} />

        <CreateProjectDialog
          directory={projectDirectoryCandidate}
          onChooseDirectory={chooseProjectDirectory}
          onCreate={handleCreateProject}
          onOpenChange={(open) => {
            setCreateProjectOpen(open);
            if (!open) clearProjectDirectory();
          }}
          open={createProjectOpen}
        />

        <ConnectionFailureDialog
          error={connectionError}
          onDismiss={() => setDismissedConnectionError(connectionErrorKey)}
          onRetry={() => {
            setDismissedConnectionError(null);
            retry();
          }}
          open={connectionFailureOpen}
        />
          </div>
        </div>
      </PiAssistantRuntimeProvider>
  );
}
