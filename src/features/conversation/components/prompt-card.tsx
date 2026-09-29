import {
  AttachmentPrimitive,
  ComposerPrimitive,
  useAui,
  useAuiState,
} from "@assistant-ui/react";
import {
  CornerDownLeftIcon,
  FolderIcon,
  GitBranchIcon,
  PlusIcon,
  SquareIcon,
  XIcon,
  ZapIcon,
} from "lucide-react";
import { useMemo, useRef, useState, type KeyboardEvent, type Ref } from "react";

import { Button } from "@/components/ui/button";
import { ModelControls } from "@/features/models";
import type { PiCommand, SelectedImage } from "@/lib/pi-rpc/client";
import type {
  ConnectionPhase,
  MessageQueue,
  ModelSnapshot,
} from "@/lib/pi-rpc/types";

import { RiskConfirmDialog } from "@/app/risk-confirm-dialog";

export function PromptCard({
  phase,
  commands,
  directoryName,
  branch,
  model,
  models,
  thinkingLevel,
  isRunning,
  isSwitchingSession = false,
  queue,
  onDraftChange,
  onStop,
  onSteer,
  onFollowUp,
  onClearQueue,
  onAddImages,
  onSelectModel,
  onSelectThinkingLevel,
  textareaRef,
}: {
  phase: ConnectionPhase;
  /** Pi 返回的扩展命令、提示词和 skills；仅用于发现和填入 `/命令`。 */
  commands: PiCommand[];
  directoryName: string | null;
  branch: string | null;
  model: ModelSnapshot | null;
  models: ModelSnapshot[];
  thinkingLevel: string | null;
  isRunning: boolean;
  /** 历史会话切换事务进行中：目标会话尚未就绪，输入区说明原因。 */
  isSwitchingSession?: boolean;
  /** Pi 待处理队列：steering 插队引导、followUp 排队后续。 */
  queue: MessageQueue;
  onDraftChange: (value: string) => void;
  onStop: () => void;
  /** 插队引导：当前工具调用结束后、下一次模型调用前交付（仅文本）。 */
  onSteer: (message: string) => void;
  /** 排队后续：当前运行完全结束后继续执行（仅文本）。 */
  onFollowUp: (message: string) => void;
  onClearQueue: () => void;
  onAddImages: () => Promise<SelectedImage[]>;
  onSelectModel: (provider: string, modelId: string) => void;
  onSelectThinkingLevel: (level: string) => void;
  textareaRef: Ref<HTMLTextAreaElement>;
}) {
  const aui = useAui();
  const attachments = useAuiState(({ composer }) => composer.attachments);
  const canSubmit = useAuiState(({ composer }) => composer.canSend);
  const composerText = useAuiState(({ composer }) => composer.text);
  const composingRef = useRef(false);
  const [isComposing, setIsComposing] = useState(false);
  const [confirmImages, setConfirmImages] = useState(false);
  const [activeCommandIndex, setActiveCommandIndex] = useState(0);

  const commandInput = composerText.startsWith("/")
    ? composerText.slice(1)
    : null;
  const commandQuery = commandInput?.toLocaleLowerCase() ?? null;
  const matchingCommands = useMemo(() => {
    if (commandQuery === null || /\s/.test(commandQuery)) return [];
    return commands.filter((command) => {
      const candidate =
        `${command.name} ${command.description}`.toLocaleLowerCase();
      return candidate.includes(commandQuery);
    });
  }, [commandQuery, commands]);
  const commandMenuOpen = commandQuery !== null && matchingCommands.length > 0;
  const activeCommand = matchingCommands.at(
    Math.min(activeCommandIndex, Math.max(matchingCommands.length - 1, 0)),
  );

  const queuedCount = queue.steering.length + queue.followUp.length;
  const canQueue = isRunning && composerText.trim().length > 0;

  /** 运行中排队：steer 插队引导、follow_up 排队后续；发送后清空输入草稿。 */
  const queueMessage = (steering: boolean) => {
    const text = composerText.trim();
    if (!canQueue || !text) return;
    if (steering) onSteer(text);
    else onFollowUp(text);
    aui.composer.setText("");
    onDraftChange("");
  };

  const isImeConfirmEnter = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    const nativeEvent = event.nativeEvent as {
      isComposing?: boolean;
      keyCode?: number;
    };
    return (
      event.key === "Enter" &&
      (composingRef.current ||
        nativeEvent.isComposing === true ||
        nativeEvent.keyCode === 229)
    );
  };

  /**
   * 在捕获阶段拦住 IME 的候选词确认键，避免 assistant-ui 把同一次 Enter 当成消息提交。
   * 只停止事件传播，不阻止浏览器默认行为，确保输入法仍能正常确认候选词。
   */
  const handleImeConfirmEnter = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (isImeConfirmEnter(event)) event.stopPropagation();
  };

  /** 运行中 Enter 视为排队发送；IME 确认键已在捕获阶段隔离。 */
  const selectCommand = (command: PiCommand) => {
    const nextText = `/${command.name} `;
    aui.composer.setText(nextText);
    onDraftChange(nextText);
    setActiveCommandIndex(0);
  };

  /** 命令菜单优先处理方向键与 Enter，不干扰正常的发送或运行中排队语义。 */
  const handleCommandNavigation = (
    event: KeyboardEvent<HTMLTextAreaElement>,
  ) => {
    if (!commandMenuOpen) return false;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const offset = event.key === "ArrowDown" ? 1 : -1;
      setActiveCommandIndex(
        (current) =>
          (current + offset + matchingCommands.length) %
          matchingCommands.length,
      );
      return true;
    }
    if (event.key === "Enter" && !event.shiftKey && activeCommand) {
      event.preventDefault();
      selectCommand(activeCommand);
      return true;
    }
    return false;
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (handleCommandNavigation(event)) return;
    if (!isRunning || event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    queueMessage(false);
  };

  const addImages = async () => {
    setConfirmImages(false);
    const images = await onAddImages();
    await Promise.all(
      images.map((image) =>
        aui.composer.addAttachment({
          type: "image",
          name: image.name,
          contentType: image.mimeType,
          content: [
            {
              type: "image",
              image: `data:${image.mimeType};base64,${image.data}`,
              filename: image.name,
            },
          ],
        }),
      ),
    );
  };

  return (
    <>
      <div className="sticky bottom-0 z-10 shrink-0 bg-background px-4 pb-5 pt-2 md:px-6">
        <div
          aria-label="任务输入卡"
          className="mx-auto w-full max-w-[720px] rounded-[20px] border border-border bg-card shadow-[0_6px_24px_rgba(0,0,0,0.05)]"
          role="group"
        >
          <div className="flex h-9 items-center gap-2 border-b border-border px-4 text-xs text-muted-foreground">
            <FolderIcon aria-hidden="true" className="size-3.5 shrink-0" />
            <span className="truncate">{directoryName ?? "默认工作目录"}</span>
            {branch && (
              <>
                <span aria-hidden="true">·</span>
                <span className="inline-flex items-center gap-1">
                  <GitBranchIcon aria-hidden="true" className="size-3.5" />
                  {branch}
                </span>
              </>
            )}
          </div>

          {attachments.length > 0 && (
            <ul className="flex flex-wrap gap-2 px-3.5 pt-2.5">
              <ComposerPrimitive.Attachments>
                {({ attachment }) => (
                  <li className="inline-flex items-center gap-1.5 rounded-lg bg-muted px-2 py-1 text-xs">
                    <span className="max-w-40 truncate">{attachment.name}</span>
                    <AttachmentPrimitive.Remove asChild>
                      <button
                        aria-label={`移除附件 ${attachment.name}`}
                        className="text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
                        type="button"
                      >
                        <XIcon aria-hidden="true" className="size-3.5" />
                      </button>
                    </AttachmentPrimitive.Remove>
                  </li>
                )}
              </ComposerPrimitive.Attachments>
            </ul>
          )}

          {queuedCount > 0 && (
            <div
              aria-label="待处理队列"
              className="flex flex-col gap-1 border-b border-border px-4 py-2"
              role="group"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs text-muted-foreground">
                  已排队 {queuedCount} 条，随当前任务进展依次交付
                </span>
                <button
                  aria-label="清空队列"
                  className="shrink-0 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
                  onClick={onClearQueue}
                  type="button"
                >
                  清空
                </button>
              </div>
              <ul className="flex flex-col gap-0.5">
                {queue.steering.map((text, index) => (
                  <li
                    className="flex items-baseline gap-1.5 text-xs"
                    key={`steering-${index}`}
                  >
                    <span className="shrink-0 text-amber-600 dark:text-amber-500">
                      插队
                    </span>
                    <span className="truncate text-muted-foreground">
                      {text}
                    </span>
                  </li>
                ))}
                {queue.followUp.map((text, index) => (
                  <li
                    className="flex items-baseline gap-1.5 text-xs"
                    key={`follow-up-${index}`}
                  >
                    <span className="shrink-0 text-muted-foreground/70">
                      排队
                    </span>
                    <span className="truncate text-muted-foreground">
                      {text}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          <ComposerPrimitive.Root>
            {commandMenuOpen && (
              <div
                aria-activedescendant={
                  activeCommand ? `pi-command-${activeCommand.name}` : undefined
                }
                aria-label="Pi 命令"
                className="mx-3.5 mt-2 max-h-48 overflow-y-auto rounded-xl border border-border bg-popover p-1 shadow-sm"
                role="listbox"
              >
                {matchingCommands.map((command, index) => (
                  <button
                    aria-selected={command.name === activeCommand?.name}
                    className="flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left text-sm outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/50 aria-selected:bg-muted"
                    id={`pi-command-${command.name}`}
                    key={`${command.source}:${command.name}`}
                    onClick={() => selectCommand(command)}
                    onMouseMove={() => setActiveCommandIndex(index)}
                    role="option"
                    type="button"
                  >
                    <span className="shrink-0 font-mono text-foreground">
                      /{command.name}
                    </span>
                    <span className="min-w-0 flex-1 text-muted-foreground">
                      {command.description}
                    </span>
                    <span className="shrink-0 text-xs text-muted-foreground/70">
                      {command.source}
                    </span>
                  </button>
                ))}
              </div>
            )}
            {/*
              焦点由本地交互决定：运行开始或滚动到底部都不应该抢走用户当前的焦点，
              否则正在编辑下一条指令或阅读执行过程的用户会被强制带走光标。
              这两个开关是 assistant-ui 的 `unstable_*` 接口，默认值为 true，
              因此这里的显式关闭由 App 测试中的焦点回归用例守护。
            */}
            <ComposerPrimitive.Input
              addAttachmentOnPaste={false}
              aria-label="任务指令"
              ref={textareaRef}
              className="max-h-48 min-h-11 w-full resize-none bg-transparent px-3.5 py-3 text-[13px] leading-5 outline-none placeholder:text-muted-foreground"
              onChange={(event) => {
                const nativeIsComposing =
                  (event.nativeEvent as { isComposing?: boolean })
                    .isComposing === true;
                if (composingRef.current || nativeIsComposing) return;
                setActiveCommandIndex(0);
                onDraftChange(event.target.value);
              }}
              onCompositionEnd={(event) => {
                composingRef.current = false;
                setIsComposing(false);
                onDraftChange(event.currentTarget.value);
              }}
              onCompositionStart={() => {
                composingRef.current = true;
                setIsComposing(true);
              }}
              onKeyDown={handleKeyDown}
              onKeyDownCapture={handleImeConfirmEnter}
              placeholder={
                isRunning
                  ? "Pi 正在执行，可排队或插队引导"
                  : isSwitchingSession
                    ? "正在打开历史会话，完成后即可发送"
                    : phase === "ready"
                      ? "描述你想完成的事情…"
                      : "连接 Pi 后即可发送"
              }
              submitMode="enter"
              unstable_focusOnRunStart={false}
              unstable_focusOnScrollToBottom={false}
            />

            <div
              aria-label="输入控制栏"
              className="flex flex-nowrap items-center gap-2 px-3 pb-2.5"
              role="group"
            >
              <Button
                aria-label="添加图片"
                onClick={() => setConfirmImages(true)}
                size="icon-sm"
                type="button"
                variant="ghost"
              >
                <PlusIcon />
              </Button>

              <ModelControls
                model={model}
                models={models}
                onSelectModel={onSelectModel}
                onSelectThinkingLevel={onSelectThinkingLevel}
                phase={phase}
                thinkingLevel={thinkingLevel}
              />

              <div className="ml-auto flex min-w-0 shrink items-center gap-2">
                <span className="hidden min-w-0 truncate text-xs text-muted-foreground/70 lg:block">
                  {isRunning
                    ? "Enter 排队发送 · 可插队引导 · 可随时停止"
                    : "Ctrl+L 选择模型 · Enter 发送 · Shift+Enter 换行"}
                </span>

                {isRunning ? (
                  <>
                    <Button
                      aria-label="插队引导"
                      disabled={!canQueue || isComposing}
                      onClick={() => queueMessage(true)}
                      size="icon-sm"
                      title="当前工具调用结束后优先处理"
                      type="button"
                      variant="ghost"
                    >
                      <ZapIcon />
                    </Button>
                    <Button
                      aria-label="排队发送"
                      disabled={!canQueue || isComposing}
                      onClick={() => queueMessage(false)}
                      size="icon-sm"
                      title="当前任务结束后继续执行"
                      type="button"
                      variant="ghost"
                    >
                      <CornerDownLeftIcon />
                    </Button>
                    <Button
                      aria-label="停止生成"
                      onClick={onStop}
                      size="icon-sm"
                      type="button"
                    >
                      <SquareIcon />
                    </Button>
                  </>
                ) : (
                  <ComposerPrimitive.Send asChild>
                    <Button
                      aria-label="发送消息"
                      disabled={!canSubmit || isComposing}
                      size="icon-sm"
                      type="button"
                    >
                      <CornerDownLeftIcon />
                    </Button>
                  </ComposerPrimitive.Send>
                )}
              </div>
            </div>
          </ComposerPrimitive.Root>
        </div>
      </div>

      <RiskConfirmDialog
        confirmLabel="继续选择图片"
        details={{
          action: "把所选图片作为附件随指令发送给 Pi。",
          recoverable:
            "本地文件不会被修改；图片内容会随指令发送给模型服务，发出后无法撤回。",
          target: "你选择的本地图片文件。",
        }}
        onCancel={() => setConfirmImages(false)}
        onConfirm={() => void addImages()}
        open={confirmImages}
        title="发送图片前确认"
      />
    </>
  );
}
