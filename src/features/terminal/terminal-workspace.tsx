import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { useEffect, useRef, useState } from "react";

import {
  getDefaultWorkspace,
  resizeTerminal,
  startTerminal,
  stopTerminal,
  TERMINAL_EXIT_EVENT,
  TERMINAL_OUTPUT_EVENT,
  type TerminalExitEvent,
  type TerminalOutputEvent,
  writeTerminal,
} from "./terminal-client";
import { TerminalTitleBar } from "./terminal-title-bar";

type TerminalPhase = "booting" | "starting" | "running" | "exited" | "error";

function errorMessage(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }
  return "未知错误";
}

export function TerminalWorkspace() {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionIdRef = useRef<number | null>(null);
  const directoryRef = useRef("");
  const startRef = useRef<(directory: string) => Promise<void>>(
    async () => undefined,
  );
  const [directory, setDirectory] = useState("");
  const [phase, setPhase] = useState<TerminalPhase>("booting");
  const [error, setError] = useState("");
  const [exitCode, setExitCode] = useState<number | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;

    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: true,
      fontFamily:
        'SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.25,
      scrollback: 10_000,
      theme: {
        background: "#191919",
        foreground: "#ececec",
        cursor: "#ececec",
        cursorAccent: "#191919",
        selectionBackground: "#4c668055",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);
    fitAddon.fit();
    terminalRef.current = terminal;
    fitRef.current = fitAddon;

    let disposed = false;
    const dimensions = () => {
      fitAddon.fit();
      const proposed = fitAddon.proposeDimensions();
      return {
        columns: Math.max(2, proposed?.cols ?? terminal.cols ?? 80),
        rows: Math.max(1, proposed?.rows ?? terminal.rows ?? 24),
      };
    };

    const start = async (nextDirectory: string) => {
      if (!nextDirectory || disposed) return;
      setPhase("starting");
      setError("");
      setExitCode(null);
      const previousSession = sessionIdRef.current;
      sessionIdRef.current = null;
      if (previousSession !== null) {
        await stopTerminal(previousSession).catch(() => undefined);
      }
      try {
        const { columns, rows } = dimensions();
        const session = await startTerminal(nextDirectory, columns, rows);
        if (disposed) {
          await stopTerminal(session.sessionId).catch(() => undefined);
          return;
        }
        sessionIdRef.current = session.sessionId;
        directoryRef.current = session.workingDirectory;
        setDirectory(session.workingDirectory);
        setPhase("running");
        terminal.focus();
      } catch (startError) {
        setError(errorMessage(startError));
        setPhase("error");
      }
    };
    startRef.current = start;

    const dataDisposable = terminal.onData((data) => {
      const sessionId = sessionIdRef.current;
      if (sessionId !== null) {
        void writeTerminal(sessionId, data).catch((writeError) => {
          setError(errorMessage(writeError));
          setPhase("error");
        });
      }
    });

    const handleResize = () => {
      const sessionId = sessionIdRef.current;
      const { columns, rows } = dimensions();
      if (sessionId !== null) {
        void resizeTerminal(sessionId, columns, rows).catch(() => undefined);
      }
    };
    window.addEventListener("resize", handleResize);

    const subscriptions = Promise.all([
      listen<TerminalOutputEvent>(TERMINAL_OUTPUT_EVENT, ({ payload }) => {
        if (payload.sessionId === sessionIdRef.current) {
          terminal.write(new Uint8Array(payload.data));
        }
      }),
      listen<TerminalExitEvent>(TERMINAL_EXIT_EVENT, ({ payload }) => {
        if (payload.sessionId !== sessionIdRef.current) return;
        sessionIdRef.current = null;
        setExitCode(payload.exitCode);
        setPhase("exited");
      }),
    ]);

    void subscriptions
      .then(() => getDefaultWorkspace())
      .then((workspace) => {
        directoryRef.current = workspace;
        setDirectory(workspace);
        return start(workspace);
      })
      .catch((startupError) => {
        setError(errorMessage(startupError));
        setPhase("error");
      });

    return () => {
      disposed = true;
      const activeSession = sessionIdRef.current;
      sessionIdRef.current = null;
      if (activeSession !== null) {
        void stopTerminal(activeSession).catch(() => undefined);
      }
      window.removeEventListener("resize", handleResize);
      dataDisposable?.dispose();
      void subscriptions.then((unlisten) =>
        unlisten.forEach((dispose) => dispose()),
      );
      terminal.dispose();
      fitAddon.dispose();
      terminalRef.current = null;
      fitRef.current = null;
    };
  }, []);

  const chooseDirectory = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") {
      directoryRef.current = selected;
      setDirectory(selected);
      await startRef.current(selected);
    }
    terminalRef.current?.focus();
  };

  const restart = () => startRef.current(directoryRef.current);
  const stop = async () => {
    const sessionId = sessionIdRef.current;
    if (sessionId === null) return;
    await stopTerminal(sessionId).catch((stopError) =>
      setError(errorMessage(stopError)),
    );
  };

  return (
    <div className="flex h-dvh min-h-0 flex-col overflow-hidden rounded-2xl border border-white/10 bg-[#191919] text-[#ececec]">
      <TerminalTitleBar
        directory={directory}
        onChooseDirectory={() => void chooseDirectory()}
        onRestart={() => void restart()}
        onStop={() => void stop()}
        running={phase === "running"}
        starting={phase === "booting" || phase === "starting"}
      />
      <main
        aria-label="Pi 终端工作区"
        className="relative min-h-0 flex-1 bg-[#191919]"
        onClick={() => terminalRef.current?.focus()}
      >
        <div className="terminal-host absolute inset-0 p-2" ref={hostRef} />
        {phase !== "running" && phase !== "booting" && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-black/20 p-6">
            <section className="pointer-events-auto w-full max-w-md rounded-xl border border-white/10 bg-[#242424]/95 p-5 shadow-2xl backdrop-blur">
              {phase === "starting" && (
                <>
                  <h1 className="text-sm font-semibold">正在启动 Pi…</h1>
                  <p className="mt-2 text-xs text-white/55">
                    正在创建 PTY 并加载当前工作目录。
                  </p>
                </>
              )}
              {phase === "exited" && (
                <>
                  <h1 className="text-sm font-semibold">
                    Pi 已退出{exitCode === null ? "" : `（代码 ${exitCode}）`}
                  </h1>
                  <p className="mt-2 text-xs text-white/55">
                    终端内容已保留，可以在相同目录重新启动。
                  </p>
                  <button
                    aria-label="重新启动 Pi"
                    className="mt-4 rounded-lg bg-white px-3 py-2 text-xs font-medium text-black"
                    onClick={() => void restart()}
                    type="button"
                  >
                    重新启动
                  </button>
                </>
              )}
              {phase === "error" && (
                <>
                  <h1 className="text-sm font-semibold">无法启动 Pi</h1>
                  <p className="mt-2 break-words text-xs text-red-200">
                    {error}
                  </p>
                  <p className="mt-2 text-xs text-white/55">
                    Pi TUI 当前不可用。请检查安装与工作目录后重试。
                  </p>
                  <div className="mt-4 flex gap-2">
                    <button
                      aria-label="重试启动"
                      className="rounded-lg bg-white px-3 py-2 text-xs font-medium text-black"
                      onClick={() => void restart()}
                      type="button"
                    >
                      重试
                    </button>
                    <button
                      className="rounded-lg border border-white/15 px-3 py-2 text-xs"
                      onClick={() => void chooseDirectory()}
                      type="button"
                    >
                      选择目录
                    </button>
                  </div>
                </>
              )}
            </section>
          </div>
        )}
      </main>
    </div>
  );
}
