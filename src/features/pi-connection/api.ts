import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import type { ConnectionSnapshot, EventEnvelope } from "./reducer";

const PI_EVENT_NAME = "lure://pi-event";

export async function selectWorkingDirectory(): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false });
  return typeof selection === "string" ? selection : null;
}

export function connectPi(workingDirectory: string): Promise<ConnectionSnapshot> {
  return invoke("connect_pi", { workingDirectory });
}

export function disconnectPi(): Promise<void> {
  return invoke("disconnect_pi");
}

export function sendPrompt(message: string): Promise<{ accepted: boolean }> {
  return invoke("send_prompt", { message });
}

export function abortPi(): Promise<void> {
  return invoke("abort_pi");
}

export function listenToPiEvents(
  onEvent: (event: EventEnvelope) => void,
): Promise<UnlistenFn> {
  return listen<EventEnvelope>(PI_EVENT_NAME, ({ payload }) => onEvent(payload));
}
