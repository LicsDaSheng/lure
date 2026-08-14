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

  // 智能贴底：新消息、流式增量与打字机揭示导致内容增长时跟随底部；
  // 用户主动上滚（离开底部 >60px）时尊重滚动位置，不强制拉回。
  React.useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const content = el.firstElementChild;
    if (!content) return;
    const nearBottom = () => el.scrollHeight - el.scrollTop - el.clientHeight < 60;
    const stick = () => {
      if (nearBottom()) el.scrollTop = el.scrollHeight;
    };
    stick();
    const obs = new MutationObserver(stick);
    obs.observe(content, {
      childList: true,
      subtree: true,
      characterData: true,
    });
    return () => obs.disconnect();
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
      <div className="mx-auto flex w-full max-w-[760px] flex-col gap-1 py-5">
        {messages.map((m) => (
          <MessageBubble key={m.id} message={m} />
        ))}
      </div>
    </ScrollArea>
  );
}
