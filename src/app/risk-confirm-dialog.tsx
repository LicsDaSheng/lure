import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

export type RiskDetails = {
  action: string;
  target: string;
  recoverable: string;
};

/**
 * 风险操作确认层：必须说明操作内容、影响对象、是否可恢复以及返回路径。
 */
export function RiskConfirmDialog({
  open,
  title,
  details,
  confirmLabel,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  details: RiskDetails;
  confirmLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent className="rounded-xl">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>请确认影响范围后再继续。</DialogDescription>
        </DialogHeader>

        <dl className="space-y-2 text-sm">
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-muted-foreground">操作内容</dt>
            <dd className="min-w-0 flex-1">{details.action}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-muted-foreground">影响对象</dt>
            <dd className="min-w-0 flex-1">{details.target}</dd>
          </div>
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-muted-foreground">可否恢复</dt>
            <dd className="min-w-0 flex-1">{details.recoverable}</dd>
          </div>
        </dl>

        <DialogFooter>
          <Button onClick={onCancel} type="button" variant="outline">
            取消
          </Button>
          <Button onClick={onConfirm} type="button">
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
