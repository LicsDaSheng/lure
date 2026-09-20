import { Button } from "@/components/ui/button";
import type { RunStatus } from "@/features/pi-connection/reducer";
import { CheckCircle2Icon, CircleDotIcon, CircleSlashIcon, HelpCircleIcon, XCircleIcon } from "lucide-react";

import { getStatusPresentation } from "./presentation";

const icons: Record<RunStatus, typeof CircleDotIcon> = {
  idle: CircleDotIcon,
  running: CircleDotIcon,
  waiting_input: HelpCircleIcon,
  completed: CheckCircle2Icon,
  failed: XCircleIcon,
  stopped: CircleSlashIcon,
};

const tones: Record<ReturnType<typeof getStatusPresentation>["tone"], string> = {
  muted: "text-muted-foreground",
  active: "text-[var(--pi-accent)]",
  warning: "text-[var(--pi-warning)]",
  success: "text-[var(--pi-success)]",
  danger: "text-[var(--pi-error)]",
};

export function RunStatusLine({
  status,
  message,
  isRunning,
  onStop,
}: {
  status: RunStatus;
  message: string;
  isRunning: boolean;
  onStop: () => void;
}) {
  const presentation = getStatusPresentation(status);
  const Icon = icons[status];

  return (
    <div
      aria-label="任务状态"
      aria-live="polite"
      className="flex flex-wrap items-center gap-2 rounded-xl border border-border/60 bg-muted/25 px-3.5 py-2.5 text-sm"
      role="status"
    >
      <Icon aria-hidden="true" className={`size-4 shrink-0 ${tones[presentation.tone]}`} />
      <span className={`font-medium ${tones[presentation.tone]}`}>{presentation.label}</span>
      {message && <span className="min-w-0 flex-1 text-muted-foreground">{message}</span>}
      {isRunning && (
        <Button onClick={onStop} size="xs" type="button" variant="outline">
          停止任务
        </Button>
      )}
    </div>
  );
}

export function RunStatusRecord({ status, content }: { status: RunStatus; content: string }) {
  const presentation = getStatusPresentation(status);
  const Icon = icons[status];
  return (
    <p className="flex items-center gap-2 px-1 text-xs text-muted-foreground">
      <Icon aria-hidden="true" className={`size-3.5 shrink-0 ${tones[presentation.tone]}`} />
      <span className={tones[presentation.tone]}>{presentation.label}</span>
      {content && <span className="min-w-0 truncate">{content}</span>}
    </p>
  );
}