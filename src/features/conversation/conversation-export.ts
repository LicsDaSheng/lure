import { messageText, type ConversationMessage } from "@/lib/pi-rpc/types";

export function serializeConversation(title: string, messages: ConversationMessage[]): string {
  const sections = messages.flatMap((message) => {
    const text = messageText(message).trim();
    if (!text) return [];
    const role = message.role === "user" ? "用户" : "Pi";
    return [`## ${role}\n\n${text}`];
  });
  return [`# ${title}`, ...sections].join("\n\n");
}
