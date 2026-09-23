import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type {
  ConversationProcessGroup,
  ConversationResult,
  ConversationTurn,
  MessagePart,
} from "@/features/conversation";

import { MarkdownResponse } from "@/components/markdown-response";
import { executionProcessLabel, toolPartToView } from "../execution-presentation";
import { ReasoningPart } from "./reasoning-part";
import { ToolCard } from "./tool-part";

/** 响应组内的过程内容按 Pi 的真实顺序渲染，工具卡保留各自的展开状态。 */
function ProcessParts({ parts, streaming }: { parts: MessagePart[]; streaming: boolean }) {
  return parts.map((part) => {
    if (part.type === "text") {
      return <MarkdownResponse key={part.id}>{part.text}</MarkdownResponse>;
    }
    if (part.type === "thinking") {
      return <ReasoningPart key={part.id} streaming={streaming} text={part.text} />;
    }
    return <ToolCard key={part.id} view={toolPartToView(part)} />;
  });
}

/** 不能称为最终答案时说明发生了什么，以及用户仍能看到什么。 */
function ResultNote({ result }: { result: ConversationResult }) {
  if (result.kind !== "partial") return null;

  if (result.errorMessage) {
    return (
      <p className="text-sm text-[var(--pi-error)]">
        这次执行未能完成：{result.errorMessage}
      </p>
    );
  }
  if (result.stopReason === "error") {
    return <p className="text-sm text-[var(--pi-error)]">这次执行未能完成。</p>;
  }
  if (result.stopReason === "length") {
    return (
      <p className="text-sm text-[var(--pi-warning)]">
        响应在完成前被截断，可以继续追问以补全结果。
      </p>
    );
  }
  if (result.stopReason === "aborted") {
    return (
      <p className="text-sm text-muted-foreground">
        任务已由你停止，已完成的内容仍然保留。
      </p>
    );
  }
  return null;
}

/** 过程组内失败或被停止的中间轮次，同样如实说明，但不充当最终结果。 */
function ProcessNote({ group }: { group: ConversationProcessGroup }) {
  if (group.stopReason === "error" || group.errorMessage) {
    return (
      <p className="text-xs text-[var(--pi-error)]">
        {group.errorMessage ? `这一次执行未能完成：${group.errorMessage}` : "这一次执行未能完成。"}
      </p>
    );
  }
  if (group.stopReason === "length") {
    return <p className="text-xs text-[var(--pi-warning)]">这一次响应被截断，随后继续执行。</p>;
  }
  if (group.stopReason === "aborted") {
    return (
      <p className="text-xs text-muted-foreground">
        任务已由你停止，已完成的内容仍然保留。
      </p>
    );
  }
  return null;
}

/**
 * 一次用户指令对应的响应组：执行过程收进可折叠区域，最终结果始终在折叠区之外。
 *
 * 运行中过程默认展开，运行结束后自动折叠；用户手动操作后的选择留在组件本地，
 * 展开某次执行不影响其他响应组。
 */
export function AssistantTurn({ turn }: { turn: ConversationTurn }) {
  const [expanded, setExpanded] = useState(turn.isRunning);
  const wasRunning = useRef(turn.isRunning);
  const contentId = useId();

  useEffect(() => {
    if (wasRunning.current === turn.isRunning) return;
    wasRunning.current = turn.isRunning;
    setExpanded(turn.isRunning);
  }, [turn.isRunning]);

  const hasProcess = turn.process.length > 0;
  const label = executionProcessLabel({
    durationMs: turn.durationMs,
    expanded,
    phase: turn.phase,
    toolCount: turn.toolCount,
  });

  // 还没有任何助手内容时不占位，避免出现空的响应组。
  if (!hasProcess && !turn.result) return null;

  return (
    <div
      aria-label="Pi 回复"
      className="flex w-full min-w-0 flex-col gap-2 px-4 py-1 text-sm text-foreground"
      data-phase={turn.phase}
    >
      {hasProcess && (
        <Collapsible
          className="group not-prose w-full"
          onOpenChange={setExpanded}
          open={expanded}
        >
          <CollapsibleTrigger
            aria-controls={contentId}
            aria-label={label}
            className="flex min-h-9 w-full items-center gap-1.5 rounded-lg pr-2 text-left text-xs text-[var(--pi-muted)] outline-none transition-colors hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50"
          >
            <ChevronRightIcon aria-hidden="true" className="size-3.5 shrink-0 group-data-[state=open]:hidden" />
            <ChevronDownIcon aria-hidden="true" className="hidden size-3.5 shrink-0 group-data-[state=open]:block" />
            <span className="min-w-0 truncate">{label}</span>
          </CollapsibleTrigger>
          {/* 折叠时保留挂载，避免销毁工具卡与思考块的局部展开状态；
              隐藏用内联 display，避免与 `flex` 的层叠顺序竞争。 */}
          <CollapsibleContent
            className="mt-1 flex flex-col gap-2 border-l border-border/70 pl-3"
            forceMount
            id={contentId}
            style={{ display: expanded ? undefined : "none" }}
          >
            {turn.process.map((group) => (
              <div className="flex flex-col gap-1.5" key={group.id}>
                <ProcessParts parts={group.parts} streaming={group.isRunning} />
                <ProcessNote group={group} />
              </div>
            ))}
          </CollapsibleContent>
        </Collapsible>
      )}

      {/* 执行过程与主结果之间以细分隔线区隔，主结果始终在折叠区之外。 */}
      {hasProcess && turn.result && (
        <hr aria-hidden="true" className="border-0 border-t border-[#E8E8EB]" />
      )}

      {turn.result && (
        <div className="flex flex-col gap-2">
          {turn.result.parts.length > 0 && (
            <MarkdownResponse>
              {turn.result.parts.map((part) => part.text).join("")}
            </MarkdownResponse>
          )}
          <ResultNote result={turn.result} />
        </div>
      )}
    </div>
  );
}
