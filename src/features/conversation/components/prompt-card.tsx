import {
  AttachmentPrimitive,
  ComposerPrimitive,
  useAui,
  useAuiState,
} from "@assistant-ui/react";
import { CornerDownLeftIcon, FolderIcon, GitBranchIcon, PlusIcon, SquareIcon, XIcon } from "lucide-react";
import { useRef, useState, type Ref } from "react";

import { Button } from "@/components/ui/button";
import { ModelControls } from "@/features/models";
import type { SelectedImage } from "@/lib/pi-rpc/client";
import type { ConnectionPhase, ModelSnapshot } from "@/lib/pi-rpc/types";

import { RiskConfirmDialog } from "@/app/risk-confirm-dialog";

export function PromptCard({
  phase,
  directoryName,
  branch,
  model,
  models,
  thinkingLevel,
  canConnectSession,
  isRunning,
  isSwitchingSession = false,
  eventsReady,
  onDraftChange,
  onStop,
  onConnect,
  onAddImages,
  onSelectModel,
  onSelectThinkingLevel,
  textareaRef,
}: {
  phase: ConnectionPhase;
  directoryName: string | null;
  branch: string | null;
  model: ModelSnapshot | null;
  models: ModelSnapshot[];
  thinkingLevel: string | null;
  canConnectSession: boolean;
  isRunning: boolean;
  /** 历史会话切换事务进行中：目标会话尚未就绪，输入区说明原因。 */
  isSwitchingSession?: boolean;
  eventsReady: boolean;
  onDraftChange: (value: string) => void;
  onStop: () => void;
  onConnect: () => void;
  onAddImages: () => Promise<SelectedImage[]>;
  onSelectModel: (provider: string, modelId: string) => void;
  onSelectThinkingLevel: (level: string) => void;
  textareaRef: Ref<HTMLTextAreaElement>;
}) {
  const aui = useAui();
  const attachments = useAuiState(({ composer }) => composer.attachments);
  const canSubmit = useAuiState(({ composer }) => composer.canSend);
  const composingRef = useRef(false);
  const [isComposing, setIsComposing] = useState(false);
  const [confirmImages, setConfirmImages] = useState(false);

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
          className="mx-auto w-full max-w-[720px] rounded-[20px] border border-[#E8E8EB] bg-card shadow-[0_6px_24px_rgba(0,0,0,0.05)]"
          role="group"
        >
          {canConnectSession && (
            <div className="flex h-9 items-center gap-2 border-b border-[#EEEEF0] px-4 text-xs text-muted-foreground">
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
              <Button
                className="ml-auto h-6 shrink-0 px-2 text-xs"
                disabled={!directoryName || !eventsReady}
                onClick={onConnect}
                type="button"
              >
                连接 Pi
              </Button>
            </div>
          )}

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

          <ComposerPrimitive.Root>
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
              className="max-h-48 min-h-11 w-full resize-none bg-transparent px-3.5 py-3 text-sm leading-6 outline-none placeholder:text-muted-foreground"
              onChange={(event) => {
                const nativeIsComposing =
                  (event.nativeEvent as { isComposing?: boolean }).isComposing === true;
                if (composingRef.current || nativeIsComposing) return;
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
              placeholder={
                isRunning
                  ? "Pi 正在执行，可继续编辑下一条消息"
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
                    ? "Ctrl+L 选择模型 · 可随时停止"
                    : "Ctrl+L 选择模型 · Enter 发送 · Shift+Enter 换行"}
                </span>

                {isRunning ? (
                  <Button aria-label="停止生成" onClick={onStop} size="icon-sm" type="button">
                    <SquareIcon />
                  </Button>
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
          recoverable: "本地文件不会被修改；图片内容会随指令发送给模型服务，发出后无法撤回。",
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
