import { LoaderCircleIcon } from "lucide-react";

/** Pi 启动期间的独立连接反馈，不进入对话记录。 */
export function ConnectionLoading() {
  return (
    <div
      aria-label="正在启动 Pi"
      aria-live="polite"
      className="m-auto flex max-w-sm flex-col items-center gap-3 text-center"
      role="status"
    >
      <span className="flex size-11 items-center justify-center rounded-full bg-primary/10 text-primary">
        <LoaderCircleIcon aria-hidden="true" className="size-5 animate-spin motion-reduce:animate-none" />
      </span>
      <div className="space-y-1">
        <p className="text-sm font-medium">正在启动 Pi</p>
        <p className="text-sm text-muted-foreground">正在加载本地工具，马上就好</p>
      </div>
    </div>
  );
}
