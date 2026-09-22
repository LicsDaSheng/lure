import { getCurrentWindow } from "@tauri-apps/api/window";
import { MinusIcon, SquareIcon, XIcon } from "lucide-react";
import type { ReactNode } from "react";

function runWindowCommand(command: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
  try {
    void command(getCurrentWindow()).catch(() => {
      // 桌面端窗口命令失败时不应中断页面交互。
    });
  } catch {
    // 浏览器预览环境没有原生窗口。
  }
}

export function WindowTitleBar() {
  return (
    <header
      aria-label="应用标题栏"
      className="window-drag-region relative z-50 flex h-10 shrink-0 select-none items-center border-b border-[#E8E8EB] bg-[#F6F6F8]"
      data-tauri-drag-region=""
    >
      <div
        className="window-drag-region flex h-full w-64 shrink-0 items-center gap-2 border-r border-[#E8E8EB] px-4"
        data-tauri-drag-region=""
      >
        <img
          alt=""
          aria-hidden="true"
          className="pointer-events-none size-5 rounded-md"
          data-tauri-drag-region=""
          src="/logo.svg"
        />
        <span
          className="pointer-events-none text-xs font-semibold tracking-tight text-foreground"
          data-tauri-drag-region=""
        >
          Lure
        </span>
      </div>

      <div className="window-drag-region min-w-0 flex-1 self-stretch" data-tauri-drag-region="" />

      <div aria-label="窗口控制" className="window-no-drag flex h-full" role="group">
        <WindowControlButton
          ariaLabel="最小化窗口"
          onClick={() => runWindowCommand((window) => window.minimize())}
        >
          <MinusIcon />
        </WindowControlButton>
        <WindowControlButton
          ariaLabel="最大化或还原窗口"
          onClick={() => runWindowCommand((window) => window.toggleMaximize())}
        >
          <SquareIcon />
        </WindowControlButton>
        <WindowControlButton
          ariaLabel="关闭窗口"
          close
          onClick={() => runWindowCommand((window) => window.close())}
        >
          <XIcon />
        </WindowControlButton>
      </div>
    </header>
  );
}

function WindowControlButton({
  ariaLabel,
  children,
  close = false,
  onClick,
}: {
  ariaLabel: string;
  children: ReactNode;
  close?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={ariaLabel}
      className={`window-no-drag flex h-10 w-11 items-center justify-center text-muted-foreground outline-none transition-colors focus-visible:z-10 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60 [&_svg]:size-3.5 [&_svg]:stroke-[1.75] ${
        close
          ? "hover:bg-[#C42B1C] hover:text-white"
          : "hover:bg-[#E9E9ED] hover:text-foreground"
      }`}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}
