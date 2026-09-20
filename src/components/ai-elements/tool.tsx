"use client";

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import type { DynamicToolUIPart, ToolUIPart } from "ai";
import {
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleIcon,
  ClockIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { isValidElement } from "react";

import { CodeBlock } from "./code-block";

export type ToolProps = ComponentProps<typeof Collapsible> & {
  status?: "running" | "completed" | "error";
};

export const Tool = ({ className, status = "running", ...props }: ToolProps) => (
  <Collapsible
    className={cn(
      "group not-prose w-full rounded-xl border border-border/60 bg-muted/25 shadow-none",
      status === "running" && "border-[var(--pi-accent)]/40",
      status === "error" && "border-[var(--pi-error)]/40 bg-[var(--pi-tool-error)]/40",
      className,
    )}
    {...props}
  />
);

export type ToolPart = ToolUIPart | DynamicToolUIPart;

export type ToolHeaderProps = {
  title?: string;
  className?: string;
} & (
  | { type: ToolUIPart["type"]; state: ToolUIPart["state"]; toolName?: never }
  | {
      type: DynamicToolUIPart["type"];
      state: DynamicToolUIPart["state"];
      toolName: string;
    }
);

const statusLabels: Record<ToolPart["state"], string> = {
  "approval-requested": "等待确认",
  "approval-responded": "已响应",
  "input-available": "执行中",
  "input-streaming": "准备中",
  "output-available": "已完成",
  "output-denied": "已取消",
  "output-error": "失败",
};

const statusIcons: Record<ToolPart["state"], ReactNode> = {
  "approval-requested": <ClockIcon className="size-4" />,
  "approval-responded": <CheckCircleIcon className="size-4" />,
  "input-available": <CircleIcon className="size-4 animate-pulse" />,
  "input-streaming": <CircleIcon className="size-4 animate-pulse" />,
  "output-available": <CheckCircleIcon className="size-4 text-[var(--pi-success)]" />,
  "output-denied": <XCircleIcon className="size-4 text-[var(--pi-warning)]" />,
  "output-error": <XCircleIcon className="size-4 text-[var(--pi-error)]" />,
};

export const getStatusBadge = (status: ToolPart["state"]) => (
  <span className="flex items-center gap-1.5 text-xs text-[var(--pi-muted)]">
    {statusIcons[status]}
    {statusLabels[status]}
  </span>
);

export const ToolHeader = ({
  className,
  title,
  type,
  state,
  toolName,
  ...props
}: ToolHeaderProps) => {
  const derivedName =
    type === "dynamic-tool" ? toolName : type.split("-").slice(1).join("-");

  return (
    <CollapsibleTrigger
      className={cn(
        "flex w-full items-center justify-between gap-4 px-4 py-2",
        className
      )}
      {...props}
    >
      <div className="flex items-center gap-2">
        <WrenchIcon className="size-4 text-[var(--pi-accent)]" />
        <span className="text-sm font-medium text-[var(--pi-accent)]">{title ?? derivedName}</span>
        {getStatusBadge(state)}
      </div>
      <>
        <ChevronRightIcon className="size-4 text-[var(--pi-muted)] group-data-[state=open]:hidden" />
        <ChevronDownIcon className="hidden size-4 text-[var(--pi-muted)] group-data-[state=open]:block" />
      </>
    </CollapsibleTrigger>
  );
};

export type ToolContentProps = ComponentProps<typeof CollapsibleContent>;

export const ToolContent = ({ className, ...props }: ToolContentProps) => (
  <CollapsibleContent
    className={cn(
      "space-y-3 border-t border-[var(--pi-dim)]/40 px-4 py-3 text-foreground outline-none",
      className
    )}
    {...props}
  />
);

export type ToolInputProps = ComponentProps<"div"> & {
  input: ToolPart["input"];
};

export const ToolInput = ({ className, input, ...props }: ToolInputProps) => (
  <div className={cn("space-y-2 overflow-hidden", className)} {...props}>
    <h4 className="font-medium text-muted-foreground text-xs tracking-wide">
      工具参数
    </h4>
    <div className="rounded-lg bg-muted/40 text-xs">
      <CodeBlock code={typeof input === "string" ? input : JSON.stringify(input, null, 2)} language="json" />
    </div>
  </div>
);

export type ToolOutputProps = ComponentProps<"div"> & {
  output: ToolPart["output"];
  errorText: ToolPart["errorText"];
};

export const ToolOutput = ({
  className,
  output,
  errorText,
  ...props
}: ToolOutputProps) => {
  if (!(output || errorText)) {
    return null;
  }

  let Output = <div>{output as ReactNode}</div>;
  let hiddenLines = 0;

  if (typeof output === "object" && !isValidElement(output)) {
    Output = (
      <CodeBlock code={JSON.stringify(output, null, 2)} language="json" />
    );
  } else if (typeof output === "string") {
    const lines = output.split("\n");
    hiddenLines = Math.max(0, lines.length - 10);
    Output = <pre className="whitespace-pre-wrap text-xs text-[var(--pi-muted)]">{lines.slice(0, 10).join("\n")}</pre>;
  }

  return (
    <div className={cn("space-y-2", className)} {...props}>
      <h4 className="font-medium text-muted-foreground text-xs tracking-wide">
        {errorText ? "错误信息" : "工具输出"}
      </h4>
      <div
        className={cn(
          "overflow-x-auto rounded-lg p-2 text-xs [&_table]:w-full",
          errorText
            ? "bg-destructive/10 text-destructive"
            : "bg-muted/50 text-foreground"
        )}
      >
        {errorText && <div>{errorText}</div>}
        {Output}
        {hiddenLines > 0 && (
          <p className="mt-1 text-xs text-[var(--pi-dim)]">… 另有 {hiddenLines} 行未显示</p>
        )}
      </div>
    </div>
  );
};
