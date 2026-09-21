import { Button } from "@/components/ui/button";
import type { LureError } from "@/features/pi-connection";
import { AlertTriangleIcon } from "lucide-react";

type ErrorCopy = {
  headline: string;
  impact: string;
  cause: string;
};

const copies: Record<string, ErrorCopy> = {
  PROCESS_EXITED: {
    headline: "Pi 进程已退出",
    impact: "当前任务无法继续执行，已完成的过程与结果仍然保留。",
    cause: "Pi 子进程结束或崩溃，桌面端已停止接收执行结果。",
  },
  RPC_PROTOCOL_ERROR: {
    headline: "Pi 通信中断",
    impact: "桌面端无法继续接收执行结果，后续指令不会发送。",
    cause: "RPC 数据流出现异常或收到无法解析的响应。",
  },
  NOT_CONNECTED: {
    headline: "没有可用的 Pi 会话",
    impact: "当前指令无法发送。",
    cause: "Pi 尚未连接或会话已经断开。",
  },
  INVALID_WORKING_DIRECTORY: {
    headline: "无法使用该工作目录",
    impact: "Pi 无法在指定目录启动，任务不能开始。",
    cause: "目录不存在，或当前账号没有访问权限。",
  },
  PI_NOT_FOUND: {
    headline: "找不到 Pi 可执行文件",
    impact: "无法启动 Pi，任务不能开始。",
    cause: "未安装 Pi，或 pi 命令不在 PATH 中。",
  },
  HANDSHAKE_TIMEOUT: {
    headline: "Pi 未在预期时间内响应",
    impact: "连接未建立，任务不能开始。",
    cause: "Pi 启动缓慢或握手过程中被中断。",
  },
  SPAWN_FAILED: {
    headline: "无法启动 Pi 进程",
    impact: "连接未建立，任务不能开始。",
    cause: "系统拒绝了进程创建请求。",
  },
  ATTACHMENT_UNREADABLE: {
    headline: "无法读取所选图片",
    impact: "图片未加入本次指令，其他内容不受影响。",
    cause: "文件不存在、无法访问，或超过可发送的大小上限。",
  },
};

const fallback: ErrorCopy = {
  headline: "操作未完成",
  impact: "当前任务暂停，已完成的过程与结果仍然保留。",
  cause: "桌面端与 Pi 之间的这次操作没有成功。",
};

export function ErrorPanel({
  error,
  canRetry,
  onRetry,
}: {
  error: LureError;
  canRetry: boolean;
  onRetry: () => void;
}) {
  const copy = copies[error.code] ?? fallback;

  return (
    <div
      className="mx-auto w-full max-w-[920px] rounded-xl border border-destructive/40 bg-destructive/5 px-4 py-3 text-sm"
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
      {canRetry && (
        <div className="mt-3 flex gap-2">
          <Button onClick={onRetry} size="sm" type="button" variant="outline">
            重新连接
          </Button>
        </div>
      )}
    </div>
  );
}