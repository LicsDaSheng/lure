import type { ToolCallMessagePartProps } from "@assistant-ui/react";
import {
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  ScrollTextIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type { ToolStatus } from "@/features/pi-connection/reducer";
import { cn } from "@/lib/utils";

import { ContentPreviewDialog } from "./content-preview-dialog";
import {
  createResultDescriptor,
  formatToolSummary,
  getToolOutputLineCount,
  type ToolView,
} from "./presentation";
import { ResultCard } from "./result-card";

/** 工具执行过程在卡片内直接显示的输出行数。 */
const PREVIEW_LINES = 10;

const statusLabels: Record<ToolStatus, string> = {
  running: "执行中",
  completed: "已完成",
  error: "失败",
};

const statusIcons: Record<ToolStatus, ReactNode> = {
  running: <CircleIcon className="size-4 animate-pulse" />,
  completed: <CheckCircleIcon className="size-4 text-[var(--pi-success)]" />,
  error: <XCircleIcon className="size-4 text-[var(--pi-error)]" />,
};

/**
 * 读取工具调用部件携带的展示数据。
 *
 * `artifact` 在 assistant-ui 契约里是 `unknown`；这里只接受
 * `toolPartToView()` 的形状，形状不符时就当没有可展示的工具，而不是当场抛错。
 */
export function readToolView(value: unknown): ToolView | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<ToolView>;
  if (typeof candidate.id !== "string" || typeof candidate.name !== "string") return null;
  const status = candidate.status;
  if (status !== "running" && status !== "completed" && status !== "error") return null;

  return {
    id: candidate.id,
    name: candidate.name,
    status,
    input: typeof candidate.input === "string" ? candidate.input : "",
    output: typeof candidate.output === "string" ? candidate.output : "",
    truncatedLines:
      typeof candidate.truncatedLines === "number" ? candidate.truncatedLines : null,
  };
}

export function ToolCard({ view }: { view: ToolView }) {
  const summary = formatToolSummary(view);
  const totalLines = getToolOutputLineCount(view);
  const result = createResultDescriptor(view);
  const lines = view.output ? view.output.split("\n") : [];
  const visible = lines.slice(0, PREVIEW_LINES).join("\n");
  const hidden = Math.max(0, lines.length - PREVIEW_LINES);

  return (
    <>
      <Collapsible
        className={cn(
          "group not-prose w-full rounded-xl border border-border/60 bg-muted/25 shadow-none",
          view.status === "running" && "border-[var(--pi-accent)]/40",
          view.status === "error" &&
            "border-[var(--pi-error)]/40 bg-[var(--pi-tool-error)]/40",
        )}
      >
        <CollapsibleTrigger className="flex w-full items-center justify-between gap-4 px-4 py-2">
          <div className="flex items-center gap-2">
            <WrenchIcon className="size-4 text-[var(--pi-accent)]" />
            <span className="text-sm font-medium text-[var(--pi-accent)]">{summary}</span>
            <span className="flex items-center gap-1.5 text-xs text-[var(--pi-muted)]">
              {statusIcons[view.status]}
              {statusLabels[view.status]}
            </span>
          </div>
          <ChevronRightIcon className="size-4 text-[var(--pi-muted)] group-data-[state=open]:hidden" />
          <ChevronDownIcon className="hidden size-4 text-[var(--pi-muted)] group-data-[state=open]:block" />
        </CollapsibleTrigger>
        <CollapsibleContent className="space-y-3 border-t border-[var(--pi-dim)]/40 px-4 py-3 text-foreground outline-none">
          {view.input && (
            <div className="space-y-2 overflow-hidden">
              <h4 className="font-medium text-muted-foreground text-xs tracking-wide">
                工具参数
              </h4>
              <pre className="overflow-x-auto rounded-lg bg-muted/40 p-2 text-xs text-[var(--pi-muted)]">
                {view.input}
              </pre>
            </div>
          )}
          {view.output && (
            <div className="space-y-2">
              <h4 className="font-medium text-muted-foreground text-xs tracking-wide">
                {view.status === "error" ? "错误信息" : "工具输出"}
              </h4>
              <div
                className={cn(
                  "overflow-x-auto rounded-lg p-2 text-xs",
                  view.status === "error"
                    ? "bg-destructive/10 text-destructive"
                    : "bg-muted/50 text-foreground",
                )}
              >
                <pre className="whitespace-pre-wrap text-xs">{visible}</pre>
                {hidden > 0 && (
                  <p className="mt-1 text-xs text-[var(--pi-dim)]">
                    … 另有 {hidden} 行未显示
                  </p>
                )}
              </div>
            </div>
          )}
          {view.output && totalLines > PREVIEW_LINES && (
            <ContentPreviewDialog
              content={view.output}
              description={`${summary} · 共 ${totalLines} 行`}
              title={summary}
              trigger={
                <Button size="xs" type="button" variant="outline">
                  <ScrollTextIcon />
                  查看完整 {totalLines} 行输出
                </Button>
              }
            />
          )}
        </CollapsibleContent>
      </Collapsible>
      {result && <ResultCard result={result} />}
    </>
  );
}

export function ToolPartCard({ artifact }: ToolCallMessagePartProps) {
  const view = readToolView(artifact);
  if (!view) return null;
  return <ToolCard view={view} />;
}