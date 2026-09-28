import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  ConnectionSnapshot,
  EventEnvelope,
  MessageQueue,
  ModelSnapshot,
  PiSessionPage,
  SessionEntries,
} from "./types";

const PI_EVENT_NAME = "lure://pi-event";

export type ImageAttachment = {
  data: string;
  mimeType: string;
};

export type PiCommand = {
  name: string;
  description: string;
  source: string;
};

export type WorkspaceContext = {
  workingDirectory: string;
  branch: string | null;
};

export type SelectedImage = ImageAttachment & { name: string };

export async function selectImageFiles(): Promise<string[]> {
  const selection = await open({
    filters: [
      { extensions: ["png", "jpg", "jpeg", "gif", "webp"], name: "图片" },
    ],
    multiple: true,
  });
  if (Array.isArray(selection)) return selection;
  return typeof selection === "string" ? [selection] : [];
}

export async function selectProjectDirectory(): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false });
  return typeof selection === "string" ? selection : null;
}

export function readImageAttachments(
  paths: string[],
): Promise<SelectedImage[]> {
  return invoke("read_image_attachments", { paths });
}

export function getDefaultWorkspace(): Promise<string> {
  return invoke("get_default_workspace");
}

/** 读取桌面进程现有会话，以便 WebView 重载后不重复创建 Pi 子进程。 */
export function getPiState(): Promise<ConnectionSnapshot> {
  return invoke("get_pi_state");
}

export function connectPi(
  workingDirectory: string,
): Promise<ConnectionSnapshot> {
  return invoke("connect_pi", { workingDirectory });
}

export function newPiSession(): Promise<ConnectionSnapshot> {
  return invoke("new_pi_session");
}

export function disconnectPi(): Promise<void> {
  return invoke("disconnect_pi");
}

export function sendPrompt(
  message: string,
  images: ImageAttachment[] = [],
): Promise<{ accepted: boolean }> {
  return invoke("send_prompt", { images, message });
}

export function abortPi(): Promise<void> {
  return invoke("abort_pi");
}

/** 运行中插队引导：当前工具调用结束后、下一次模型调用前交付。 */
export function steerPi(
  message: string,
  images: ImageAttachment[] = [],
): Promise<{ accepted: boolean }> {
  return invoke("steer_pi", { images, message });
}

/** 运行中排队后续：当前运行完全结束后继续执行。 */
export function followUpPi(
  message: string,
  images: ImageAttachment[] = [],
): Promise<{ accepted: boolean }> {
  return invoke("follow_up_pi", { images, message });
}

/** 清空待处理队列，返回被清空的内容。 */
export function clearPiQueue(): Promise<MessageQueue> {
  return invoke("clear_pi_queue");
}

export function getAvailableModels(): Promise<ModelSnapshot[]> {
  return invoke("get_available_models");
}

export function selectModel(
  provider: string,
  modelId: string,
): Promise<ModelSnapshot> {
  return invoke("set_model", { modelId, provider });
}

export function selectThinkingLevel(level: string): Promise<string> {
  return invoke("set_thinking_level", { level });
}

export function getPiCommands(): Promise<PiCommand[]> {
  return invoke("get_commands");
}

export function respondToExtensionUi(
  requestId: string,
  value: unknown,
  cancelled: boolean,
): Promise<void> {
  return invoke("respond_extension_ui", { cancelled, requestId, value });
}

export function getWorkspaceContext(
  workingDirectory: string,
): Promise<WorkspaceContext> {
  return invoke("get_workspace_context", { workingDirectory });
}

/** 列出某个工作目录中已记录的 Pi 会话，只读扫描，不要求 Pi 已连接。 */
export function listProjectSessions(
  workingDirectory: string,
  offset: number,
  limit: number,
): Promise<PiSessionPage> {
  return invoke("list_project_sessions", { limit, offset, workingDirectory });
}

/** 切换到已记录的 Pi 会话；`switched` 为 false 表示 Pi 扩展取消了这次切换。 */
export function switchPiSession(
  sessionPath: string,
): Promise<SessionSwitchOutcome> {
  return invoke("switch_pi_session", { sessionPath });
}

/** 读取当前会话的完整条目，用于重建历史对话。 */
export function getSessionEntries(): Promise<SessionEntries> {
  return invoke("get_session_entries");
}

export type SessionSwitchOutcome = {
  switched: boolean;
  snapshot: ConnectionSnapshot;
};

export function listenToPiEvents(
  onEvent: (event: EventEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<EventEnvelope>(PI_EVENT_NAME, ({ payload }) =>
    onEvent(payload),
  );
}
