import { getCurrentWindow } from "@tauri-apps/api/window";
import { FolderOpenIcon, RotateCwIcon, SquareIcon } from "lucide-react";

function runWindowCommand(
  command: (window: ReturnType<typeof getCurrentWindow>) => Promise<void>,
) {
  try {
    void command(getCurrentWindow()).catch(() => undefined);
  } catch {
    // 浏览器预览环境没有原生窗口。
  }
}

export function TerminalTitleBar({
  directory,
  running,
  starting,
  onChooseDirectory,
  onRestart,
  onStop,
}: {
  directory: string;
  running: boolean;
  starting: boolean;
  onChooseDirectory: () => void;
  onRestart: () => void;
  onStop: () => void;
}) {
  return (
    <header
      aria-label="应用标题栏"
      className="window-drag-region flex h-12 shrink-0 select-none items-center border-b border-black/10 bg-[#ebdbb2] px-2 text-[#3c3836]"
      data-tauri-drag-region=""
    >
      <WindowControls />
      <div
        className="window-drag-region ml-3 flex min-w-0 flex-1 items-center gap-2"
        data-tauri-drag-region=""
      >
        <span className="text-xs font-semibold tracking-wide">Lure</span>
        <span aria-hidden="true" className="text-[#3c3836]/25">
          /
        </span>
        <span className="truncate text-xs text-[#7c6f64]" title={directory}>
          {directory || "正在读取工作目录…"}
        </span>
      </div>
      <div className="window-no-drag flex items-center gap-1">
        <TitleButton
          ariaLabel="选择工作目录"
          disabled={starting}
          icon={<FolderOpenIcon />}
          onClick={onChooseDirectory}
        />
        <TitleButton
          ariaLabel="重新启动 Pi"
          disabled={starting || !directory}
          icon={<RotateCwIcon className={starting ? "animate-spin" : ""} />}
          onClick={onRestart}
        />
        <TitleButton
          ariaLabel="停止 Pi"
          disabled={!running || starting}
          icon={<SquareIcon />}
          onClick={onStop}
        />
      </div>
    </header>
  );
}

function WindowControls() {
  return (
    <div aria-label="窗口控制" className="window-no-drag flex" role="group">
      <WindowButton
        ariaLabel="关闭窗口"
        color="bg-[#FF5F57]"
        onClick={() => runWindowCommand((window) => window.close())}
      />
      <WindowButton
        ariaLabel="最小化窗口"
        color="bg-[#FEBC2E]"
        onClick={() => runWindowCommand((window) => window.minimize())}
      />
      <WindowButton
        ariaLabel="最大化或还原窗口"
        color="bg-[#28C840]"
        onClick={() => runWindowCommand((window) => window.toggleMaximize())}
      />
    </div>
  );
}

function WindowButton({
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
      className="flex h-9 w-6 items-center justify-center rounded-md outline-none focus-visible:ring-2 focus-visible:ring-[#458588]"
      onClick={onClick}
      type="button"
    >
      <span className={`size-3.5 rounded-full ${color} ring-1 ring-black/25`} />
    </button>
  );
}

function TitleButton({
  ariaLabel,
  disabled,
  icon,
  onClick,
}: {
  ariaLabel: string;
  disabled: boolean;
  icon: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={ariaLabel}
      className="flex size-9 items-center justify-center rounded-lg text-[#504945] outline-none transition-colors hover:bg-black/10 hover:text-[#3c3836] focus-visible:ring-2 focus-visible:ring-[#458588] disabled:cursor-not-allowed disabled:text-[#928374]/50 [&_svg]:size-4 [&_svg]:stroke-[1.75]"
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      {icon}
    </button>
  );
}
