import {
  ThreadPrimitive,
  useAui,
  type AppendMessage,
} from "@assistant-ui/react";
import { useAppDispatch } from "@/app/hooks";
import { Button } from "@/components/ui/button";
import { readImageAttachments, selectImageFiles } from "@/lib/pi-rpc";
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
import { SettingsPage, useThemePreference } from "@/features/preferences";
import {
  ConnectionFailureDialog,
  CreateProjectDialog,
  ErrorPanel,
  sessionsActions,
  sessionTitle,
  TaskNavigation,
  useSessions,
} from "@/features/sessions";
import { WindowTitleBar } from "@/app/window-title-bar";
import { ArrowDownIcon, LoaderCircleIcon } from "lucide-react";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

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
    return () => {
      dispatch(sessionsActions.eventSubscriptionReleased());
    };
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
  const {
    messages,
    activeAssistantId,
    commands,
    promptSubmissionCount,
    queue,
    prompt,
    abort,
    steer,
    followUp,
    clearQueue,
  } = useConversation();
  const {
    availableModels,
    current: currentModel,
    thinkingLevel,
    setModel,
    setThinkingLevel,
  } = useModels();
  const { request: extensionRequest, respond: respondToExtension } =
    useExtensionUi();
  const submittedPrompts = useRef(promptSubmissionCount);
  const [composerGeneration, setComposerGeneration] = useState(0);
  const [createProjectOpen, setCreateProjectOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [navigationView, setNavigationView] = useState<
    "conversations" | "projects"
  >("conversations");
  const lastProjectDirectoryRef = useRef<string | null>(null);
  const lastProjectSessionRef = useRef<typeof activeSessionSummary>(null);
  const tabTransitionObservedRef = useRef(false);
  const [tabTransitionStart, setTabTransitionStart] = useState<{
    directory: string | null;
    sessionId: string | null;
  } | null>(null);
  const [dismissedConnectionError, setDismissedConnectionError] = useState<
    string | null
  >(null);
  const promptInputRef = useRef<HTMLTextAreaElement>(null);
  const { preference: theme, setPreference: setTheme } = useThemePreference();

  const directory = connection.workingDirectory ?? selectedDirectory;
  const directoryName = directoryNameOf(directory);
  const branch = workspaceContext?.branch ?? null;
  // “当前会话”锚定在事务事实（activeSessionSummary）上，而不是连接快照：
  // 连接快照可能被迟到的事件短暂覆盖，用它判定选中态会让列表闪现兑底条目。
  const activeSessionId = activeSessionSummary?.id ?? connection.sessionId;

  useEffect(() => {
    if (
      !directory ||
      !projects.some((project) => project.directory === directory)
    )
      return;
    lastProjectDirectoryRef.current = directory;
    lastProjectSessionRef.current = activeSessionSummary;
  }, [activeSessionSummary, directory, projects]);

  // 草稿按工作目录保存在本机，切换任务或意外关闭后可以恢复。
  const persistDraft = useCallback(
    (value: string) => writeDraft(selectedDirectory, value),
    [selectedDirectory],
  );

  // 会话摘要是 Pi 对话标题的唯一事实来源；顶栏与导航复用同一标题规则。
  const taskTitle = activeSessionSummary
    ? sessionTitle(activeSessionSummary, directoryName ?? "新任务")
    : (directoryName ?? "新任务");
  const hasConversation = messages.length > 0;
  const isRunning = connection.phase === "running";
  // 会话切换事务未结束时不能发送：目标会话的内容尚未确定。
  const canSend = connection.phase === "ready" && !sessionTransition;
  const switchingSession = Boolean(sessionTransition);
  const tabTransitionPending = tabTransitionStart !== null;
  // 连接失败只在发生时不请自来地提醒一次，关闭后不反复打扰。
  const connectionErrorKey = connectionError
    ? `${connectionError.code}:${connectionError.message}`
    : null;
  const connectionFailureOpen =
    connectionErrorKey !== null &&
    connectionErrorKey !== dismissedConnectionError;
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
    if (target) writeDraft(target, "");
    setComposerGeneration((value) => value + 1);
  }, [defaultWorkspace, newDefaultConversation, selectedDirectory]);

  const handleNewProjectTask = useCallback(
    (targetDirectory: string) => {
      lastProjectDirectoryRef.current = targetDirectory;
      lastProjectSessionRef.current = null;
      newProjectConversation(targetDirectory);
      writeDraft(targetDirectory, "");
      setComposerGeneration((value) => value + 1);
    },
    [newProjectConversation],
  );

  const handleOpenConversation = useCallback(
    (session: Parameters<typeof openConversation>[0]) => {
      const sessionDirectory = session.cwd;
      if (
        sessionDirectory &&
        projects.some((project) => project.directory === sessionDirectory)
      ) {
        lastProjectDirectoryRef.current = sessionDirectory;
        lastProjectSessionRef.current = session;
      }
      openConversation(session);
    },
    [openConversation, projects],
  );

  const handleNavigationViewChange = useCallback(
    (view: "conversations" | "projects") => {
      if (view === navigationView) return;
      tabTransitionObservedRef.current = false;
      setTabTransitionStart({
        directory,
        sessionId: connection.sessionId,
      });
      setNavigationView(view);
      if (view === "conversations") {
        void handleNewTask();
        return;
      }

      const lastProjectSession = lastProjectSessionRef.current;
      if (lastProjectSession) {
        handleOpenConversation(lastProjectSession);
        return;
      }
      const lastProjectDirectory = lastProjectDirectoryRef.current;
      if (lastProjectDirectory) {
        handleNewProjectTask(lastProjectDirectory);
        return;
      }
      void handleNewTask();
    },
    [
      handleNewProjectTask,
      handleNewTask,
      handleOpenConversation,
      connection.sessionId,
      directory,
      navigationView,
    ],
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

  useEffect(() => {
    if (!tabTransitionStart) return;
    if (connection.phase === "connecting" || sessionTransition) {
      tabTransitionObservedRef.current = true;
      return;
    }
    const identityChanged =
      directory !== tabTransitionStart.directory ||
      connection.sessionId !== tabTransitionStart.sessionId;
    const settled = connection.phase === "ready" && !sessionTransition;
    if ((identityChanged || tabTransitionObservedRef.current) && settled) {
      setTabTransitionStart(null);
      return;
    }
    if (connectionError || error) setTabTransitionStart(null);
  }, [
    connection.phase,
    connection.sessionId,
    connectionError,
    directory,
    error,
    sessionTransition,
    tabTransitionStart,
  ]);

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
          hasConversation={!settingsOpen && hasConversation}
          onToggleSidebar={() => setSidebarCollapsed((collapsed) => !collapsed)}
          title={taskTitle}
        />
        <div className="relative flex min-h-0 flex-1 overflow-hidden overscroll-none">
          {settingsOpen ? (
            <SettingsPage
              onBack={() => setSettingsOpen(false)}
              onThemeChange={setTheme}
              theme={theme}
            />
          ) : (
            <>
              {!sidebarCollapsed && (
                <TaskNavigation
                  activeView={navigationView}
                  activeDirectory={directory}
                  activeSessionId={activeSessionId}
                  canOpenConversation={canSend}
                  defaultWorkspace={defaultWorkspace}
                  disabled={
                    connection.phase === "running" ||
                    connection.phase === "connecting" ||
                    switchingSession
                  }
                  expandedProjects={expandedProjects}
                  hasTask={Boolean(directory)}
                  loadingDirectories={loadingDirectories}
                  onNewTask={() => void handleNewTask()}
                  onNewProject={() => setCreateProjectOpen(true)}
                  onNewProjectTask={handleNewProjectTask}
                  onActiveViewChange={handleNavigationViewChange}
                  onOpenConversation={handleOpenConversation}
                  onOpenSettings={() => {
                    setSidebarCollapsed(false);
                    setSettingsOpen(true);
                  }}
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

              <main
                aria-label="任务工作区"
                aria-busy={tabTransitionPending}
                className="relative flex min-w-0 flex-1 flex-col"
              >
                {tabTransitionPending && (
                  <div
                    aria-label="正在切换页面"
                    className="absolute inset-0 z-40 flex items-center justify-center bg-background/80 backdrop-blur-[1px]"
                    role="status"
                  >
                    <div className="flex items-center gap-2.5 rounded-xl border border-border bg-card px-4 py-3 text-sm text-muted-foreground shadow-sm">
                      <LoaderCircleIcon
                        aria-hidden="true"
                        className="size-5 animate-spin motion-reduce:animate-none"
                        strokeWidth={1.75}
                      />
                      正在加载页面…
                    </div>
                  </div>
                )}
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
                      commands={commands}
                      directoryName={directoryName}
                      isRunning={isRunning}
                      isSwitchingSession={switchingSession}
                      model={currentModel}
                      models={availableModels}
                      onAddImages={pickImages}
                      onClearQueue={() => clearQueue()}
                      onDraftChange={persistDraft}
                      onFollowUp={(message) => followUp(message)}
                      onSelectModel={(provider, modelId) =>
                        void setModel(provider, modelId)
                      }
                      onSelectThinkingLevel={(level) =>
                        void setThinkingLevel(level)
                      }
                      onSteer={(message) => steer(message)}
                      onStop={() => void abort()}
                      phase={connection.phase}
                      queue={queue}
                      textareaRef={promptInputRef}
                      thinkingLevel={thinkingLevel}
                    />
                  </ThreadPrimitive.Viewport>
                </ThreadPrimitive.Root>
              </main>
            </>
          )}

          <ExtensionUiDialog
            onRespond={(value, cancelled) => {
              respondToExtension(value, cancelled);
              return Promise.resolve();
            }}
            request={extensionRequest}
          />

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
