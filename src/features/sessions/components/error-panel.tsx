import { Button } from "@/components/ui/button";
import type { LureError } from "@/lib/pi-rpc/types";
import { AlertTriangleIcon } from "lucide-react";

import { commandFallback, errorCopy } from "./error-copy";

/**
 * 非连接类错误的内联说明；连接失败改用模态提醒，不在此重复显示。
 */
export function ErrorPanel({
  error,
  onDismiss,
}: {
  error: LureError;
  onDismiss: () => void;
}) {
  const copy = errorCopy(error, commandFallback);

  return (
    <div
      className="mx-auto w-full max-w-[720px] rounded-xl border border-destructive/40 bg-destructive/5 px-4 py-3 text-sm"
      role="alert"
    >
      <p className="flex items-center gap-2 font-medium text-destructive">
        <AlertTriangleIcon aria-hidden="true" className="size-4 shrink-0" />
        {copy.headline}
      </p>
      <dl className="mt-2 space-y-1 text-muted-foreground">
        <div className="flex gap-1.5">
          <dt className="shrink-0 text-foreground/80">影响：</dt>
          <dd>{copy.impact}</dd>
        </div>
        <div className="flex gap-1.5">
          <dt className="shrink-0 text-foreground/80">原因：</dt>
          <dd>
            {copy.cause}
            {error.message && <span className="ml-1 opacity-70">（{error.message}）</span>}
          </dd>
        </div>
      </dl>
      <div className="mt-3 flex gap-2">
        <Button onClick={onDismiss} size="sm" type="button" variant="outline">
          知道了
        </Button>
      </div>
    </div>
  );
}