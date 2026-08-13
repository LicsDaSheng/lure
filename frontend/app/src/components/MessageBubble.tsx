import * as React from "react";
import { Sparkles, User, ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";
import { Markdown } from "./Markdown";
import { useTypewriter } from "@/hooks/useTypewriter";
import type { UiMessage } from "@/hooks/useChat";

export function MessageBubble({ message }: { message: UiMessage }) {
  const isUser = message.role === "user";
  const typewriter = !isUser && !!message.typewriter;
  const reasoning = message.reasoning ?? "";
  const revealedReasoning = useTypewriter(reasoning, typewriter);
  const reasoningTyping =
    typewriter && revealedReasoning.length < reasoning.length;
  const revealedContent = useTypewriter(
    message.content,
    typewriter,
    2,
    24,
    !reasoningTyping,
  );
  const contentTyping =
    typewriter && revealedContent.length < message.content.length;
  return (
    <div
      className={cn(
        "group flex gap-3 px-4 py-3",
        isUser ? "flex-row-reverse" : "flex-row",
      )}
    >
      <div
        className={cn(
          "mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-full",
          isUser
            ? "bg-user-bubble text-user-bubble-foreground"
            : "bg-muted text-accent",
        )}
      >
        {isUser ? <User className="size-4" /> : <Sparkles className="size-4" />}
      </div>
      <div
        className={cn(
          "flex min-w-0 max-w-[min(680px,80%)] flex-col gap-1",
          isUser ? "items-end" : "items-start",
        )}
      >
        {reasoning ? (
          <ReasoningBlock
            text={revealedReasoning}
            streaming={message.streaming}
            typewriter={message.typewriter}
            typing={reasoningTyping}
          />
        ) : null}
        <div
          className={cn(
            "rounded-2xl px-4 py-2.5 text-[0.925rem] leading-relaxed",
            isUser
              ? "rounded-tr-sm bg-user-bubble text-user-bubble-foreground"
              : "rounded-tl-sm bg-card text-card-foreground shadow-sm ring-1 ring-border",
          )}
        >
          {isUser ? (
            <span className="whitespace-pre-wrap break-words">
              {message.content}
            </span>
          ) : (
            <StreamingMarkdown
              content={revealedContent}
              typing={contentTyping}
            />
          )}
        </div>
      </div>
    </div>
  );
}

/** assistant 消息渲染：新生成回答用打字机逐字揭示 + 尾部光标；历史消息（无 typewriter 标记）全文直显。 */
function StreamingMarkdown({
  content,
  typing,
}: {
  content: string;
  typing: boolean;
}) {
  return (
    <div data-testid="assistant-message-content">
      <Markdown>{content}</Markdown>
      {typing ? <span className="typewriter-cursor" aria-hidden /> : null}
    </div>
  );
}

function ReasoningBlock({
  text,
  streaming,
  typewriter,
  typing,
}: {
  text: string;
  streaming?: boolean;
  typewriter?: boolean;
  typing: boolean;
}) {
  const [open, setOpen] = React.useState(
    () => !!typewriter && !!streaming,
  );
  return (
    <div className="w-full">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
      >
        <ChevronRight
          className={cn("size-3 transition-transform", open && "rotate-90")}
        />
        思维链
      </button>
      {open ? (
        <div
          data-testid="assistant-reasoning-content"
          className="mt-1 whitespace-pre-wrap rounded-lg border border-dashed bg-muted/40 px-3 py-2 font-mono text-xs leading-relaxed text-muted-foreground"
        >
          {text}
          {typing ? <span className="typewriter-cursor" aria-hidden /> : null}
        </div>
      ) : null}
    </div>
  );
}
