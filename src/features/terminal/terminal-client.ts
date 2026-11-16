import { invoke } from "@tauri-apps/api/core";

export const TERMINAL_OUTPUT_EVENT = "lure://terminal-output";
export const TERMINAL_EXIT_EVENT = "lure://terminal-exit";

export interface TerminalSession {
  sessionId: number;
  workingDirectory: string;
}

export interface TerminalOutputEvent {
  sessionId: number;
  data: number[];
}

export interface TerminalExitEvent {
  sessionId: number;
  exitCode: number | null;
  success: boolean;
}

export function getDefaultWorkspace(): Promise<string> {
  return invoke("get_default_workspace");
}

export function startTerminal(
  workingDirectory: string,
  columns: number,
  rows: number,
): Promise<TerminalSession> {
  return invoke("start_terminal", { workingDirectory, columns, rows });
}

export function writeTerminal(sessionId: number, data: string): Promise<void> {
  return invoke("write_terminal", { sessionId, data });
}

export function resizeTerminal(
  sessionId: number,
  columns: number,
  rows: number,
): Promise<void> {
  return invoke("resize_terminal", { sessionId, columns, rows });
}

export function stopTerminal(sessionId: number): Promise<void> {
  return invoke("stop_terminal", { sessionId });
}
