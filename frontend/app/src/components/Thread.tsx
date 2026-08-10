import * as React from "react";
import { Sparkles } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { MessageBubble } from "./MessageBubble";
import type { UiMessage } from "@/hooks/useChat";

export function Thread({
  messages,
  modelName,
}: {
  messages: UiMessage[];
  modelName: string | null;
}) {
  const viewportRef = React.useRef<HTMLDivElement>(null);

  // 新消息 / 流式增量时贴底。
  React.useEffect(() => {
    const el = viewportRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 text-center">
        <div className="flex size-12 items-center justify-center rounded-2xl bg-muted text-accent">
          <Sparkles className="size-6" />
        </div>
        <h2 className="text-lg font-semibold tracking-tight">开始一段对话</h2>
        <p className="max-w-sm text-sm text-muted-foreground">
          {modelName
            ? `当前模型 ${modelName}。输入消息即可开始。`
            : "输入消息即可开始。"}
        </p>
      </div>
    );
  }

  return (
    <ScrollArea className="flex-1" viewportRef={viewportRef}>
      <div className="mx-auto flex max-w-[820px] flex-col py-4">
        {messages.map((m) => (
          <MessageBubble key={m.id} message={m} />
        ))}
      </div>
    </ScrollArea>
  );
}
