import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { LureError } from "@/lib/pi-rpc/types";

import { connectionFallback, errorCopy } from "./error-copy";

/**
 * 连接失败提醒：连接过程本身静默执行，只有失败时才用模态要求用户处理。
 */
export function ConnectionFailureDialog({
  open,
  error,
  onDismiss,
  onRetry,
}: {
  open: boolean;
  error: LureError | null;
  onDismiss: () => void;
  onRetry: () => void;
}) {
  if (!error) return null;
  const copy = errorCopy(error, connectionFallback);

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onDismiss()}>
      <DialogContent className="rounded-xl">
        <DialogHeader>
          <DialogTitle>{copy.headline}</DialogTitle>
          <DialogDescription>Pi 连接没有建立，任务暂时无法开始。</DialogDescription>
        </DialogHeader>

        <dl className="space-y-2 text-sm">
          <div className="flex gap-2">
            <dt className="w-14 shrink-0 text-muted-foreground">影响：</dt>
            <dd className="min-w-0 flex-1">{copy.impact}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="w-14 shrink-0 text-muted-foreground">原因：</dt>
            <dd className="min-w-0 flex-1">
              {copy.cause}
              {error.message && <span className="ml-1 opacity-70">（{error.message}）</span>}
            </dd>
          </div>
        </dl>

        <DialogFooter>
          <Button onClick={onDismiss} type="button" variant="outline">
            稍后再说
          </Button>
          <Button onClick={onRetry} type="button">
            重新连接
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}