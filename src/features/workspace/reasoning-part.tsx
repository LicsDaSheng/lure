import type { MessagePartStatus } from "@assistant-ui/react";
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";

import { MarkdownResponse } from "./markdown-response";

type ReasoningPartProps = {
  text: string;
  status: MessagePartStatus;
};

/**
 * 思考过程展示。
 *
 * Pi 不提供思考耗时，因此耗时按部件从流式到结束的实际时间估算；
 * 历史消息没有这个过程时保留中性文案。
 */
export function ReasoningPart({ text, status }: ReasoningPartProps) {
  const streaming = status.type === "running";
  const [duration, setDuration] = useState<number | undefined>(undefined);
  const startedAt = useRef<number | null>(null);

  useEffect(() => {
    if (streaming) {
      startedAt.current ??= Date.now();
      return;
    }
    if (startedAt.current !== null) {
      setDuration(Math.max(1, Math.ceil((Date.now() - startedAt.current) / 1000)));
      startedAt.current = null;
    }
  }, [streaming]);

  const label = streaming
    ? "思考中…"
    : duration === undefined
      ? "思考过程"
      : `已思考 ${duration} 秒`;

  return (
    <Collapsible className="group not-prose" defaultOpen={false}>
      <CollapsibleTrigger className="flex w-full items-center gap-1 text-sm italic text-[var(--pi-muted)] transition-colors hover:text-foreground">
        <ChevronRightIcon className="size-4 group-data-[state=open]:hidden" />
        <ChevronDownIcon className="hidden size-4 group-data-[state=open]:block" />
        {label}
      </CollapsibleTrigger>
      <CollapsibleContent className="pi-markdown mt-1 pl-4 text-sm italic text-[var(--pi-muted)] outline-none">
        <MarkdownResponse>{text}</MarkdownResponse>
      </CollapsibleContent>
    </Collapsible>
  );
}