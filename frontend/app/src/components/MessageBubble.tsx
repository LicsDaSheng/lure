import * as React from "react";
import { Check, ChevronRight, LoaderCircle, Wrench } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";
import { Markdown, markdownComponents } from "./Markdown";
import { useTypewriter } from "@/hooks/useTypewriter";
import {
  formatElapsed,
  useElapsedSeconds,
  useMeasuredDuration,
} from "@/lib/activity-timer";
import { separateGluedReasoningBlocks } from "@/lib/reasoning-blocks";
import type { UiMessage } from "@/hooks/useChat";

export function MessageBubble({ message }: { message: UiMessage }) {
  const isUser = message.role === "user";
  const typewriter = !isUser && !!message.typewriter;
  const reasoning = separateGluedReasoningBlocks(message.reasoning ?? "");
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

  if (isUser) {
    return (
      <div className="group px-4 py-1.5" data-role="user">
        <div
          className="w-full rounded-xl border border-border/80 bg-card/75 px-3 py-2 text-[0.875rem] leading-6 text-foreground/95 shadow-[0_1px_0_color-mix(in_oklch,var(--foreground)_4%,transparent)] backdrop-blur-sm"
          data-slot="user-message"
        >
          <span className="whitespace-pre-wrap break-words">
            {message.content}
          </span>
        </div>
      </div>
    );
  }

  return (
    <div
      className="group px-4 py-2.5"
      data-role="assistant"
    >
      <div
        className="flex min-w-0 w-full flex-col gap-2 text-pretty"
        data-slot="assistant-message"
      >
        {reasoning.trim() ? (
          <ReasoningBlock
            message={message}
            revealed={revealedReasoning}
            typing={reasoningTyping}
          />
        ) : null}
        {message.tools?.map((tool) => (
          <ToolRow key={tool.id} name={tool.name} status={tool.status} />
        ))}
        {message.content || contentTyping ? (
          <div className="min-w-0 text-[0.875rem] leading-6 text-foreground">
            <StreamingMarkdown
              content={revealedContent}
              typing={contentTyping}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

function ToolRow({
  name,
  status,
}: {
  name: string;
  status: "running" | "complete";
}) {
  const running = status === "running";
  return (
    <div
      className="flex min-h-5 items-center gap-1.5 text-[0.72rem] leading-5 text-muted-foreground/80"
      data-slot="tool-row"
      data-status={status}
    >
      <span className="grid size-3.5 shrink-0 place-items-center">
        {running ? (
          <LoaderCircle className="size-3 animate-spin" aria-label="工具运行中" />
        ) : (
          <Wrench className="size-3" aria-hidden />
        )}
      </span>
      <span>{running ? "正在运行" : "已运行"}</span>
      <code className="font-mono text-[0.68rem] text-muted-foreground">
        {name}
      </code>
      {!running ? <Check className="size-3 text-emerald-700/70" aria-label="工具已完成" /> : null}
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
  message,
  revealed,
  typing,
}: {
  message: UiMessage;
  revealed: string;
  typing: boolean;
}) {
  const reasoning = message.reasoning ?? "";
  const pending =
    !!message.streaming && !message.reasoningDone && reasoning.trim().length > 0;
  const [userOpen, setUserOpen] = React.useState<boolean | null>(null);
  const open = userOpen ?? pending;
  const isPreview = pending && userOpen === null;

  const timerKey = `reasoning:${message.id}:0`;
  const elapsed = useElapsedSeconds(pending, timerKey);
  const measured = useMeasuredDuration(pending, timerKey);

  // live preview 期间把滚动容器钉在底部，最新 token 始终可见。
  const scrollRef = React.useRef<HTMLDivElement | null>(null);
  React.useEffect(() => {
    if (!isPreview) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [isPreview, revealed]);

  let label: string;
  if (pending) {
    label = "思考中";
  } else if (measured === null) {
    label = "已思考";
  } else if (measured < 1) {
    label = "快速思考";
  } else {
    label = `思考了 ${formatElapsed(measured)}`;
  }

  return (
    <div className="w-full">
      <button
        onClick={() => setUserOpen(!open)}
        aria-expanded={open}
        data-slot="reasoning-toggle"
        className="flex min-h-5 items-center gap-1 text-[0.72rem] text-muted-foreground/80 transition-colors hover:text-foreground"
      >
        <ChevronRight
          className={cn("size-3 transition-transform", open && "rotate-90")}
        />
        <span className={cn(pending && "shimmer")} data-slot="reasoning-label">
          {label}
        </span>
        {pending ? (
          <span className="tabular-nums text-muted-foreground/60">{elapsed}s</span>
        ) : null}
      </button>
      {open ? (
        <div
          ref={scrollRef}
          data-testid="assistant-reasoning-content"
          className={cn("mt-0.5 pb-1", isPreview && "max-h-40 overflow-y-auto")}
        >
          <div className="text-xs leading-snug text-muted-foreground/85">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              components={markdownComponents}
            >
              {revealed.trimStart()}
            </ReactMarkdown>
            {typing ? <span className="typewriter-cursor" aria-hidden /> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
