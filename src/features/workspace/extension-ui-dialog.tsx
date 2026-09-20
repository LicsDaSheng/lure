import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { ExtensionUiRequest } from "@/features/pi-connection/reducer";
import { useEffect, useRef, useState } from "react";

export function ExtensionUiDialog({
  request,
  onRespond,
}: {
  request: ExtensionUiRequest | null;
  onRespond: (value: unknown, cancelled?: boolean) => Promise<void>;
}) {
  const [value, setValue] = useState("");
  const submitted = useRef(false);

  useEffect(() => {
    setValue(request?.defaultValue ?? request?.options[0] ?? "");
    submitted.current = false;
  }, [request]);

  if (!request) return null;

  const submit = async (resolvedValue: unknown) => {
    submitted.current = true;
    await onRespond(resolvedValue, false);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !submitted.current) void onRespond(null, true);
      }}
    >
      <DialogContent className="rounded-xl">
        <DialogHeader>
          <DialogTitle>{request.title ?? "Pi 需要你的输入"}</DialogTitle>
          <DialogDescription>
            {request.message ?? "请提供继续执行所需的信息。"}
          </DialogDescription>
        </DialogHeader>

        {request.method === "select" && (
          <label className="grid gap-2 text-sm">
            <span>选择一项</span>
            <select
              aria-label="选择一项"
              className="h-10 rounded-lg border bg-background px-3"
              onChange={(event) => setValue(event.target.value)}
              value={value}
            >
              {request.options.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </select>
          </label>
        )}

        {(request.method === "input" || request.method === "editor") && (
          <label className="grid gap-2 text-sm">
            <span>{request.method === "editor" ? "编辑内容" : "输入内容"}</span>
            <textarea
              aria-label={request.method === "editor" ? "编辑内容" : "输入内容"}
              className="min-h-24 resize-y rounded-lg border bg-background p-3 outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              onChange={(event) => setValue(event.target.value)}
              placeholder={request.placeholder ?? undefined}
              value={value}
            />
          </label>
        )}

        <DialogFooter>
          <Button onClick={() => void onRespond(null, true)} variant="outline">
            取消
          </Button>
          {request.method === "confirm" ? (
            <Button onClick={() => void submit(true)}>确认操作</Button>
          ) : (
            <Button disabled={!value.trim()} onClick={() => void submit(value)}>
              提交输入
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
