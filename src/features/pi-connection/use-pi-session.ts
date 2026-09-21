import { useCallback, useEffect, useReducer, useRef, useState } from "react";

import { writeLastDirectory } from "@/features/workspace/local-preferences";

import {
  abortPi,
  connectPi,
  disconnectPi,
  getAvailableModels,
  getDefaultWorkspace,
  getPiCommands,
  getWorkspaceContext,
  listenToPiEvents,
  newPiSession,
  respondToExtensionUi,
  selectModel,
  selectThinkingLevel,
  sendPrompt,
  type ImageAttachment,
  type PiCommand,
  type WorkspaceContext,
} from "./api";
import {
  disconnectedSnapshot,
  initialPiSessionState,
  piSessionReducer,
  type EventEnvelope,
  type LureError,
  type ModelSnapshot,
} from "./reducer";

export function usePiSession() {
  const [state, dispatch] = useReducer(piSessionReducer, initialPiSessionState);
  const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<LureError | null>(null);
  const [eventsReady, setEventsReady] = useState(false);
  const [availableModels, setAvailableModels] = useState<ModelSnapshot[]>([]);
  const [commands, setCommands] = useState<PiCommand[]>([]);
  const [workspaceContext, setWorkspaceContext] = useState<WorkspaceContext | null>(null);
  const lastSequence = useRef(0);
  const startupAttempted = useRef(false);

  /**
   * Pi 事件按序号单调递增，桌面端据此忽略重复投递。
   *
   * 事件订阅在开发模式的重复挂载或注册竞态下可能短暂存在两个活跃监听，
   * 去重可以保证同一个 RPC 事件只被应用一次。
   */
  const handlePiEvent = useCallback((event: EventEnvelope) => {
    if (event.sequence !== 0) {
      if (event.sequence <= lastSequence.current) return;
      lastSequence.current = event.sequence;
    }
    dispatch(event);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenToPiEvents((event) => {
      if (!disposed) handlePiEvent(event);
    })
      .then((dispose) => {
        if (disposed) dispose();
        else {
          unlisten = dispose;
          setEventsReady(true);
        }
      })
      .catch((error: unknown) => {
        setCommandError(normalizeError(error, "无法监听 Pi RPC 事件"));
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handlePiEvent]);

  const loadWorkspaceContext = useCallback(async (directory: string) => {
    try {
      const context = await getWorkspaceContext(directory);
      setWorkspaceContext(
        context ?? { branch: null, workingDirectory: directory },
      );
    } catch {
      setWorkspaceContext({ branch: null, workingDirectory: directory });
    }
  }, []);

  const refreshCapabilities = useCallback(async () => {
    const [modelsResult, commandsResult] = await Promise.allSettled([
      getAvailableModels(),
      getPiCommands(),
    ]);
    if (modelsResult.status === "fulfilled" && Array.isArray(modelsResult.value)) {
      setAvailableModels(modelsResult.value);
    }
    if (commandsResult.status === "fulfilled" && Array.isArray(commandsResult.value)) {
      setCommands(commandsResult.value);
    }
  }, []);

  const connect = useCallback(async () => {
    if (!selectedDirectory || !eventsReady) return;
    setCommandError(null);
    try {
      const snapshot = await connectPi(selectedDirectory);
      dispatch({ sequence: 0, event: { type: "session_ready", snapshot } });
      await Promise.all([
        refreshCapabilities(),
        loadWorkspaceContext(snapshot.workingDirectory ?? selectedDirectory),
      ]);
    } catch (error) {
      setCommandError(normalizeError(error, "连接 Pi 失败"));
    }
  }, [eventsReady, loadWorkspaceContext, refreshCapabilities, selectedDirectory]);

  const connectDefault = useCallback(async (): Promise<boolean> => {
    if (!eventsReady) return false;
    setCommandError(null);
    try {
      const directory = await getDefaultWorkspace();
      setSelectedDirectory(directory);
      writeLastDirectory(directory);
      const snapshot = await connectPi(directory);
      dispatch({ sequence: 0, event: { type: "session_ready", snapshot } });
      await Promise.all([
        refreshCapabilities(),
        loadWorkspaceContext(snapshot.workingDirectory ?? directory),
      ]);
      return true;
    } catch (error) {
      setCommandError(normalizeError(error, "启动默认 Pi RPC 失败"));
      return false;
    }
  }, [eventsReady, loadWorkspaceContext, refreshCapabilities]);

  useEffect(() => {
    if (!eventsReady || startupAttempted.current) return;
    startupAttempted.current = true;
    void connectDefault();
  }, [connectDefault, eventsReady]);

  const newConversation = useCallback(async () => {
    if (state.connection.phase === "disconnected" || state.connection.phase === "failed") {
      return connectDefault();
    }
    setCommandError(null);
    try {
      const snapshot = await newPiSession();
      dispatch({ sequence: 0, event: { type: "session_ready", snapshot } });
      const directory = snapshot.workingDirectory ?? selectedDirectory;
      if (directory) {
        setSelectedDirectory(directory);
        await loadWorkspaceContext(directory);
      }
      await refreshCapabilities();
      return true;
    } catch (error) {
      setCommandError(normalizeError(error, "新建对话失败"));
      return false;
    }
  }, [connectDefault, loadWorkspaceContext, refreshCapabilities, selectedDirectory, state.connection.phase]);

  const disconnect = useCallback(async () => {
    setCommandError(null);
    try {
      await disconnectPi();
      dispatch({
        sequence: 0,
        event: { type: "connection_changed", snapshot: disconnectedSnapshot },
      });
      setAvailableModels([]);
      setCommands([]);
    } catch (error) {
      setCommandError(normalizeError(error, "断开 Pi 失败"));
    }
  }, []);

  const prompt = useCallback(async (message: string, images: ImageAttachment[] = []) => {
    setCommandError(null);
    try {
      await sendPrompt(message, images);
    } catch (error) {
      const normalized = normalizeError(error, "发送消息失败");
      setCommandError(normalized);
      throw normalized;
    }
  }, []);

  const abort = useCallback(async () => {
    setCommandError(null);
    try {
      await abortPi();
    } catch (error) {
      setCommandError(normalizeError(error, "停止运行失败"));
    }
  }, []);

  const setModel = useCallback(async (provider: string, modelId: string) => {
    setCommandError(null);
    try {
      const model = await selectModel(provider, modelId);
      dispatch({
        sequence: 0,
        event: {
          type: "connection_changed",
          snapshot: { ...state.connection, model },
        },
      });
    } catch (error) {
      setCommandError(normalizeError(error, "切换模型失败"));
    }
  }, [state.connection]);

  const setThinkingLevel = useCallback(async (level: string) => {
    setCommandError(null);
    try {
      const thinkingLevel = await selectThinkingLevel(level);
      dispatch({
        sequence: 0,
        event: {
          type: "connection_changed",
          snapshot: { ...state.connection, thinkingLevel },
        },
      });
    } catch (error) {
      setCommandError(normalizeError(error, "切换思考强度失败"));
    }
  }, [state.connection]);

  const respondToExtension = useCallback(
    async (value: unknown, cancelled = false) => {
      const request = state.extensionRequest;
      if (!request) return;
      setCommandError(null);
      try {
        await respondToExtensionUi(request.requestId, value, cancelled);
      } catch (error) {
        setCommandError(normalizeError(error, "无法提交交互响应"));
      }
    },
    [state.extensionRequest],
  );

  return {
    state,
    selectedDirectory,
    eventsReady,
    error: commandError ?? state.error,
    availableModels,
    commands,
    workspaceContext,
    connect,
    newConversation,
    disconnect,
    prompt,
    abort,
    setModel,
    setThinkingLevel,
    respondToExtension,
  };
}

function normalizeError(error: unknown, fallback: string): LureError {
  if (typeof error === "object" && error !== null) {
    const candidate = error as Partial<LureError>;
    if (typeof candidate.code === "string" && typeof candidate.message === "string") {
      return { code: candidate.code, message: candidate.message };
    }
  }
  if (error instanceof Error) {
    return { code: "CLIENT_ERROR", message: error.message };
  }
  return { code: "CLIENT_ERROR", message: fallback };
}
