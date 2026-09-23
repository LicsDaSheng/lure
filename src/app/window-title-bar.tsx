import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FolderIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
} from "lucide-react";

function runWindowCommand(command: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
  try {
    void command(getCurrentWindow()).catch(() => {
      // 桌面端窗口命令失败时不应中断页面交互。
    });
  } catch {
    // 浏览器预览环境没有原生窗口。
  }
}

export function WindowTitleBar({
  collapsed,
  hasConversation,
  onTitleChange,
  onToggleSidebar,
  title,
}: {
  collapsed: boolean;
  hasConversation: boolean;
  onTitleChange: (title: string) => void;
  onToggleSidebar: () => void;
  title: string;
}) {
  const sidebarToggle = (
    <button
      aria-label={collapsed ? "展开左侧栏" : "折叠左侧栏"}
      className="window-no-drag flex size-9 shrink-0 items-center justify-center rounded-lg text-muted-foreground outline-none transition-colors hover:bg-[var(--button-subtle-hover)] hover:text-foreground active:bg-[var(--button-subtle-active)] focus-visible:ring-2 focus-visible:ring-ring/60 [&_svg]:size-4 [&_svg]:stroke-[1.75]"
      onClick={onToggleSidebar}
      type="button"
    >
      {collapsed ? <PanelLeftOpenIcon /> : <PanelLeftCloseIcon />}
    </button>
  );

  return (
    <header
      aria-label="应用标题栏"
      className="window-drag-region relative z-50 flex h-12 shrink-0 select-none items-stretch"
      data-tauri-drag-region=""
    >
      {!collapsed && (
        <div
          className="window-drag-region flex h-full w-[272px] shrink-0 items-center border-r border-border bg-sidebar px-2"
          data-tauri-drag-region=""
        >
          <WindowControls />
          <div className="ml-1">{sidebarToggle}</div>
        </div>
      )}

      <div
        aria-label={hasConversation ? "任务顶栏" : undefined}
        className="window-drag-region flex min-w-0 flex-1 items-center gap-2 border-b border-border bg-background px-4 md:px-5"
        data-tauri-drag-region=""
      >
        {collapsed && (
          <>
            <WindowControls />
            <div className="ml-1">{sidebarToggle}</div>
          </>
        )}
        {hasConversation && (
          <>
            <FolderIcon aria-hidden="true" className="pointer-events-none hidden size-4 shrink-0 text-muted-foreground sm:block" />
            <input
              aria-label="任务标题"
              className="window-no-drag min-w-0 w-full max-w-[480px] truncate bg-transparent text-[13px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              onChange={(event) => onTitleChange(event.target.value)}
              value={title}
            />
          </>
        )}
        <div className="min-w-0 flex-1 self-stretch" data-tauri-drag-region="" />
      </div>
    </header>
  );
}

function WindowControls() {
  return (
    <div aria-label="窗口控制" className="window-no-drag flex items-center" role="group">
      <WindowControlButton
        ariaLabel="关闭窗口"
        color="bg-[#FF5F57]"
        onClick={() => runWindowCommand((window) => window.close())}
      />
      <WindowControlButton
        ariaLabel="最小化窗口"
        color="bg-[#FEBC2E]"
        onClick={() => runWindowCommand((window) => window.minimize())}
      />
      <WindowControlButton
        ariaLabel="最大化或还原窗口"
        color="bg-[#28C840]"
        onClick={() => runWindowCommand((window) => window.toggleMaximize())}
      />
    </div>
  );
}

function WindowControlButton({
  ariaLabel,
  color,
  onClick,
}: {
  ariaLabel: string;
  color: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={ariaLabel}
      className="window-no-drag group flex h-9 w-6 items-center justify-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      onClick={onClick}
      type="button"
    >
      <span
        aria-hidden="true"
        className={`size-3.5 rounded-full ${color} ring-1 ring-black/10 transition-transform group-hover:scale-110`}
      />
    </button>
  );
}
