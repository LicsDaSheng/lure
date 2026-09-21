import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { ConnectionSnapshot, EventEnvelope, ModelSnapshot } from "./pi-session-types";

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
    filters: [{ extensions: ["png", "jpg", "jpeg", "gif", "webp"], name: "图片" }],
    multiple: true,
  });
  if (Array.isArray(selection)) return selection;
  return typeof selection === "string" ? [selection] : [];
}

export async function selectProjectDirectory(): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false });
  return typeof selection === "string" ? selection : null;
}

export function readImageAttachments(paths: string[]): Promise<SelectedImage[]> {
  return invoke("read_image_attachments", { paths });
}

export function getDefaultWorkspace(): Promise<string> {
  return invoke("get_default_workspace");
}

/** 读取桌面进程现有会话，以便 WebView 重载后不重复创建 Pi 子进程。 */
export function getPiState(): Promise<ConnectionSnapshot> {
  return invoke("get_pi_state");
}

export function connectPi(workingDirectory: string): Promise<ConnectionSnapshot> {
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

export function getAvailableModels(): Promise<ModelSnapshot[]> {
  return invoke("get_available_models");
}

export function selectModel(provider: string, modelId: string): Promise<ModelSnapshot> {
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

export function getWorkspaceContext(workingDirectory: string): Promise<WorkspaceContext> {
  return invoke("get_workspace_context", { workingDirectory });
}

export function listenToPiEvents(
  onEvent: (event: EventEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<EventEnvelope>(PI_EVENT_NAME, ({ payload }) => onEvent(payload));
}
