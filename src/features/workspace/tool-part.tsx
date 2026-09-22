import {
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  ScrollTextIcon,
  XCircleIcon,
} from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import type { ToolStatus } from "@/features/pi-connection";
import { cn } from "@/lib/utils";

import { ContentPreviewDialog } from "./content-preview-dialog";
import {
  formatToolSummary,
  getToolOutputLineCount,
  type ToolView,
} from "./presentation";

/** 工具执行过程在卡片内直接显示的输出行数。 */
const PREVIEW_LINES = 10;

const statusLabels: Record<ToolStatus, string> = {
  running: "执行中",
  completed: "已完成",
  error: "失败",
};

const statusIcons: Record<ToolStatus, ReactNode> = {
  running: <CircleIcon className="size-3.5 animate-pulse" />,
  completed: <CheckCircleIcon className="size-3.5 text-[var(--pi-success)]" />,
  error: <XCircleIcon className="size-3.5 text-[var(--pi-error)]" />,
};

function toolOperation(view: ToolView): string {
  try {
    const input = JSON.parse(view.input) as Record<string, unknown>;
    const operation = ["command", "operation", "action"].find(
      (key) => typeof input[key] === "string" && input[key],
    );
    if (operation && input[operation] !== view.name) {
      return `${view.name} · ${input[operation] as string}`;
    }
  } catch {
    // 非 JSON 参数仍可显示工具名。
  }
  return view.name;
}

export function ToolCard({ view }: { view: ToolView }) {
  const summary = formatToolSummary(view);
  const totalLines = getToolOutputLineCount(view);
  const lines = view.output ? view.output.split("\n") : [];
  const visible = lines.slice(0, PREVIEW_LINES).join("\n");
  const hidden = Math.max(0, lines.length - PREVIEW_LINES);

  return (
    <Collapsible className="group not-prose w-full">
        <CollapsibleTrigger
          aria-label={`${summary}，${statusLabels[view.status]}`}
          className="flex min-h-7 w-full min-w-0 items-center gap-2 py-1 text-left text-[var(--pi-muted)] transition-colors hover:text-foreground"
        >
          <ChevronRightIcon className="size-4 shrink-0 group-data-[state=open]:hidden" />
          <ChevronDownIcon className="hidden size-4 shrink-0 group-data-[state=open]:block" />
          <span className="shrink-0 text-sm font-medium text-foreground">工具调用</span>
          <span aria-hidden="true" className="shrink-0 text-[var(--pi-dim)]">·</span>
          <span className="min-w-0 truncate text-sm">{toolOperation(view)}</span>
          <span className="flex shrink-0 items-center gap-1 text-xs">
            <span aria-hidden="true">·</span>
            {statusIcons[view.status]}
            {statusLabels[view.status]}
          </span>
        </CollapsibleTrigger>
        <CollapsibleContent className="mt-1.5 overflow-hidden rounded-xl border border-border/50 bg-muted/40 text-foreground outline-none">
          {view.input && (
            <div className="grid grid-cols-[3rem_minmax(0,1fr)] items-start gap-3 px-4 py-3">
              <h4 className="pt-0.5 text-xs font-medium text-[var(--pi-dim)]">输入</h4>
              <pre className="min-w-0 overflow-x-auto whitespace-pre-wrap break-words font-mono text-xs leading-5 text-[var(--pi-muted)]">
                {view.input}
              </pre>
            </div>
          )}
          {view.output && (
            <div className="grid grid-cols-[3rem_minmax(0,1fr)] items-start gap-3 border-t border-border/60 px-4 py-3">
              <h4 className="pt-0.5 text-xs font-medium text-[var(--pi-dim)]">
                {view.status === "error" ? "错误" : "输出"}
              </h4>
              <div
                className={cn(
                  "min-w-0 overflow-x-auto text-xs",
                  view.status === "error"
                    ? "text-destructive"
                    : "text-[var(--pi-muted)]",
                )}
              >
                <pre className="whitespace-pre-wrap break-words font-mono text-xs leading-5">{visible}</pre>
                {hidden > 0 && (
                  <p className="mt-1 text-xs text-[var(--pi-dim)]">
                    … 另有 {hidden} 行未显示
                  </p>
                )}
              </div>
            </div>
          )}
          {view.output && totalLines > PREVIEW_LINES && (
            <div className="border-t border-border/60 px-4 py-2 pl-[4.75rem]">
              <ContentPreviewDialog
                content={view.output}
                description={`${summary} · 共 ${totalLines} 行`}
                title={summary}
                trigger={
                  <Button size="xs" type="button" variant="ghost">
                    <ScrollTextIcon />
                    查看完整 {totalLines} 行输出
                  </Button>
                }
              />
            </div>
          )}
        </CollapsibleContent>
    </Collapsible>
  );
}
