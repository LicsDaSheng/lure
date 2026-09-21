import {
  AttachmentPrimitive,
  ComposerPrimitive,
  useAui,
  useAuiState,
} from "@assistant-ui/react";
import { CornerDownLeftIcon, FolderIcon, GitBranchIcon, PlusIcon, SquareIcon, XIcon } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type Ref } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { SelectedImage } from "@/features/pi-connection";
import type { ConnectionPhase, ModelSnapshot } from "@/features/pi-connection";
import { cn } from "@/lib/utils";

import { RiskConfirmDialog } from "./risk-confirm-dialog";

const thinkingLevels = ["off", "minimal", "low", "medium", "high"] as const;

export function PromptCard({
  phase,
  directoryName,
  branch,
  model,
  models,
  thinkingLevel,
  canConnectSession,
  isRunning,
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
  const [modelPickerOpen, setModelPickerOpen] = useState(false);
  const [activeProvider, setActiveProvider] = useState("");
  const [pendingModelKey, setPendingModelKey] = useState("");

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

  const modelOptions = models.length ? models : model ? [model] : [];
  const modelGroups = new Map<string, ModelSnapshot[]>();
  for (const option of modelOptions) {
    const group = modelGroups.get(option.provider) ?? [];
    group.push(option);
    modelGroups.set(option.provider, group);
  }
  const modelKey = model ? `${model.provider}::${model.id}` : "";
  const selectedModelKey = modelOptions.some(
    (option) => `${option.provider}::${option.id}` === modelKey,
  )
    ? modelKey
    : modelOptions[0]
      ? `${modelOptions[0].provider}::${modelOptions[0].id}`
      : "";

  const openModelPicker = useCallback(() => {
    const selected = modelOptions.find(
      (option) => `${option.provider}::${option.id}` === selectedModelKey,
    ) ?? modelOptions[0];
    if (!selected) return;

    setActiveProvider(selected.provider);
    setPendingModelKey(`${selected.provider}::${selected.id}`);
    setModelPickerOpen(true);
  }, [modelOptions, selectedModelKey]);

  const confirmModelSelection = () => {
    const selected = modelOptions.find(
      (option) => `${option.provider}::${option.id}` === pendingModelKey,
    );
    if (!selected) return;

    setModelPickerOpen(false);
    onSelectModel(selected.provider, selected.id);
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.repeat ||
        !event.ctrlKey ||
        event.altKey ||
        event.metaKey ||
        event.key.toLowerCase() !== "l" ||
        modelPickerOpen ||
        modelOptions.length === 0
      ) {
        return;
      }

      event.preventDefault();
      openModelPicker();
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [modelOptions.length, modelPickerOpen, openModelPicker]);

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

              {modelOptions.length > 0 && (
                <select
                  aria-label="模型"
                  className={cn(
                    "h-7 w-44 min-w-0 max-w-[35%] truncate rounded-lg bg-transparent px-2 text-xs text-muted-foreground outline-none",
                    "hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50",
                  )}
                  onChange={(event) => {
                    const [provider, modelId] = event.target.value.split("::");
                    if (provider && modelId) onSelectModel(provider, modelId);
                  }}
                  value={selectedModelKey}
                >
                  {[...modelGroups].map(([provider, options]) => (
                    <optgroup key={provider} label={provider}>
                      {options.map((option) => (
                        <option
                          key={`${option.provider}::${option.id}`}
                          value={`${option.provider}::${option.id}`}
                        >
                          {option.id} [{option.provider}]
                        </option>
                      ))}
                    </optgroup>
                  ))}
                </select>
              )}

              <select
                aria-label="思考强度"
                className="h-7 rounded-lg bg-transparent px-2 text-xs text-muted-foreground outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring/50"
                disabled={phase !== "ready"}
                onChange={(event) => onSelectThinkingLevel(event.target.value)}
                value={thinkingLevel ?? "medium"}
              >
                {thinkingLevels.map((level) => (
                  <option key={level} value={level}>
                    思考 {level}
                  </option>
                ))}
              </select>

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

      <Dialog open={modelPickerOpen} onOpenChange={setModelPickerOpen}>
        <DialogContent className="rounded-xl" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>选择模型</DialogTitle>
            <DialogDescription>选择一个模型后，点击确认才会切换。</DialogDescription>
          </DialogHeader>
          <div aria-label="模型提供方" className="flex gap-1 overflow-x-auto border-b" role="tablist">
            {[...modelGroups].map(([provider, options]) => (
              <button
                aria-controls={`model-provider-${provider}`}
                aria-selected={provider === activeProvider}
                className={cn(
                  "shrink-0 border-b-2 px-3 py-2 text-sm text-muted-foreground",
                  provider === activeProvider
                    ? "border-primary text-foreground"
                    : "border-transparent hover:text-foreground",
                )}
                id={`model-provider-tab-${provider}`}
                key={provider}
                onClick={() => {
                  setActiveProvider(provider);
                  const selectedInProvider = options.some(
                    (option) => `${option.provider}::${option.id}` === pendingModelKey,
                  );
                  if (!selectedInProvider) {
                    setPendingModelKey(`${options[0]?.provider}::${options[0]?.id}`);
                  }
                }}
                role="tab"
                type="button"
              >
                {provider}
              </button>
            ))}
          </div>
          {[...modelGroups].map(([provider, options]) =>
            provider === activeProvider ? (
              <div
                aria-labelledby={`model-provider-tab-${provider}`}
                className="max-h-72 space-y-2 overflow-y-auto"
                id={`model-provider-${provider}`}
                key={provider}
                role="tabpanel"
              >
                <div aria-label="可用模型" className="grid gap-2" role="radiogroup">
                  {options.map((option) => {
                    const optionKey = `${option.provider}::${option.id}`;
                    const selected = optionKey === pendingModelKey;
                    return (
                      <button
                        aria-checked={selected}
                        className={cn(
                          "rounded-lg border px-3 py-2 text-left text-sm",
                          selected
                            ? "border-primary bg-accent text-foreground"
                            : "border-border hover:bg-accent/50",
                        )}
                        key={optionKey}
                        onClick={() => setPendingModelKey(optionKey)}
                        role="radio"
                        type="button"
                      >
                        {option.id}
                      </button>
                    );
                  })}
                </div>
              </div>
            ) : null,
          )}
          <DialogFooter>
            <Button onClick={() => setModelPickerOpen(false)} type="button" variant="outline">
              取消
            </Button>
            <Button onClick={confirmModelSelection} type="button">
              确认选择
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
