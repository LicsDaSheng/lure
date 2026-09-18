import { useCallback, useEffect, useReducer, useState } from "react";

import {
  abortPi,
  connectPi,
  disconnectPi,
  listenToPiEvents,
  selectWorkingDirectory,
  sendPrompt,
} from "./api";
import {
  disconnectedSnapshot,
  initialPiSessionState,
  piSessionReducer,
  type LureError,
} from "./reducer";

export function usePiSession() {
  const [state, dispatch] = useReducer(piSessionReducer, initialPiSessionState);
  const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
  const [commandError, setCommandError] = useState<LureError | null>(null);
  const [eventsReady, setEventsReady] = useState(false);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listenToPiEvents((event) => dispatch(event))
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
  }, []);

  const chooseDirectory = useCallback(async () => {
    setCommandError(null);
    try {
      const directory = await selectWorkingDirectory();
      if (directory) setSelectedDirectory(directory);
    } catch (error) {
      setCommandError(normalizeError(error, "无法选择工作目录"));
    }
  }, []);

  const connect = useCallback(async () => {
    if (!selectedDirectory || !eventsReady) return;
    setCommandError(null);
    try {
      const snapshot = await connectPi(selectedDirectory);
      dispatch({ sequence: 0, event: { type: "session_ready", snapshot } });
    } catch (error) {
      setCommandError(normalizeError(error, "连接 Pi 失败"));
    }
  }, [eventsReady, selectedDirectory]);

  const disconnect = useCallback(async () => {
    setCommandError(null);
    try {
      await disconnectPi();
      dispatch({
        sequence: 0,
        event: { type: "connection_changed", snapshot: disconnectedSnapshot },
      });
    } catch (error) {
      setCommandError(normalizeError(error, "断开 Pi 失败"));
    }
  }, []);

  const prompt = useCallback(async (message: string) => {
    setCommandError(null);
    try {
      await sendPrompt(message);
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

  return {
    state,
    selectedDirectory,
    eventsReady,
    error: commandError ?? state.error,
    chooseDirectory,
    connect,
    disconnect,
    prompt,
    abort,
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
