import { ComposerPrimitive, useAui } from "@assistant-ui/react";
import { Button } from "@/components/ui/button";
import type { ConnectionPhase, ModelSnapshot } from "@/features/pi-connection/reducer";
import { CornerDownLeftIcon, FolderIcon, GitBranchIcon, PlusIcon, SquareIcon, XIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { useLayoutEffect, type Ref } from "react";

export type PromptAttachment = {
  name: string;
  data: string;
  mimeType: string;
};

const thinkingLevels = ["off", "minimal", "low", "medium", "high"] as const;

const connectionLabels: Record<ConnectionPhase, string> = {
  disconnected: "未连接",
  connecting: "正在连接",
  ready: "已连接",
  running: "执行中",
  failed: "连接失败",
};

export function PromptCard({
  phase,
  directoryName,
  branch,
  model,
  models,
  thinkingLevel,
  draft,
  attachments,
  canConnectSession,
  canSubmit,
  isRunning,
  eventsReady,
  onDraftChange,
  onStop,
  onChooseDirectory,
  onConnect,
  onAddImages,
  onRemoveAttachment,
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
  draft: string;
  attachments: PromptAttachment[];
  canConnectSession: boolean;
  canSubmit: boolean;
  isRunning: boolean;
  eventsReady: boolean;
  onDraftChange: (value: string) => void;
  onStop: () => void;
  onChooseDirectory: () => void;
  onConnect: () => void;
  onAddImages: () => void;
  onRemoveAttachment: (name: string) => void;
  onSelectModel: (provider: string, modelId: string) => void;
  onSelectThinkingLevel: (level: string) => void;
  textareaRef: Ref<HTMLTextAreaElement>;
}) {
  const aui = useAui();
  useLayoutEffect(() => {
    if (aui.composer.getState().text !== draft) {
      aui.composer.setText(draft);
    }
  }, [aui, draft]);

  const modelOptions = models.length
    ? models
    : model
      ? [model]
      : [];
  const modelGroups = new Map<string, ModelSnapshot[]>();
  for (const option of modelOptions) {
    const group = modelGroups.get(option.provider) ?? [];
    group.push(option);
    modelGroups.set(option.provider, group);
  }
  const modelKey = model ? `${model.provider}::${model.id}` : "";
  const selectedModelKey =
    modelOptions.some((option) => `${option.provider}::${option.id}` === modelKey)
      ? modelKey
      : modelOptions[0]
        ? `${modelOptions[0].provider}::${modelOptions[0].id}`
        : "";

  return (
    <div className="shrink-0 px-4 pb-4 pt-2 md:px-6">
      <div
        aria-label="任务输入卡"
        className="mx-auto w-full max-w-[920px] rounded-[20px] border border-border/60 bg-card shadow-sm"
        role="group"
      >
        <div className="flex items-center gap-2 border-b border-border/50 px-3.5 py-2 text-xs text-muted-foreground">
          <FolderIcon aria-hidden="true" className="size-3.5 shrink-0" />
          <span className="truncate">{directoryName ?? "尚未选择工作目录"}</span>
          {branch && (
            <>
              <span aria-hidden="true">·</span>
              <span className="inline-flex items-center gap-1">
                <GitBranchIcon aria-hidden="true" className="size-3.5" />
                {branch}
              </span>
            </>
          )}
          <span aria-hidden="true">·</span>
          <span>{connectionLabels[phase]}</span>
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <Button
              className="h-6 px-2 text-xs"
              onClick={onChooseDirectory}
              type="button"
              variant="ghost"
            >
              {directoryName ? "更换目录" : "选择工作目录"}
            </Button>
            {canConnectSession && (
              <Button
                className="h-6 px-2 text-xs"
                disabled={!directoryName || !eventsReady}
                onClick={onConnect}
                type="button"
              >
                连接 Pi
              </Button>
            )}
          </div>
        </div>

        {attachments.length > 0 && (
          <ul className="flex flex-wrap gap-2 px-3.5 pt-2.5">
            {attachments.map((attachment) => (
              <li
                className="inline-flex items-center gap-1.5 rounded-lg bg-muted px-2 py-1 text-xs"
                key={attachment.name}
              >
                <span className="max-w-40 truncate">{attachment.name}</span>
                <button
                  aria-label={`移除附件 ${attachment.name}`}
                  className="text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50"
                  onClick={() => onRemoveAttachment(attachment.name)}
                  type="button"
                >
                  <XIcon aria-hidden="true" className="size-3.5" />
                </button>
              </li>
            ))}
          </ul>
        )}

        <ComposerPrimitive.Root>
          <ComposerPrimitive.Input
            addAttachmentOnPaste={false}
            aria-label="任务指令"
            ref={textareaRef}
            className="max-h-48 min-h-11 w-full resize-none bg-transparent px-3.5 py-3 text-sm leading-6 outline-none placeholder:text-muted-foreground"
            onChange={(event) => onDraftChange(event.target.value)}
            placeholder={
              isRunning
                ? "Pi 正在执行，可继续编辑下一条消息"
                : phase === "ready"
                  ? "描述你想完成的事情…"
                  : "选择工作目录并连接 Pi 后即可发送"
            }
            submitMode="enter"
            unstable_focusOnRunStart={false}
            unstable_focusOnScrollToBottom={false}
          />

          <div className="flex flex-wrap items-center gap-2 px-3 pb-2.5">
            <Button
              aria-label="添加图片"
              onClick={onAddImages}
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
                  "h-7 max-w-48 rounded-lg bg-transparent px-2 text-xs text-muted-foreground outline-none",
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

            <span className="ml-auto hidden text-xs text-muted-foreground sm:block">
              {isRunning ? "可随时停止" : "Enter 发送 · Shift+Enter 换行"}
            </span>

            {isRunning ? (
              <Button aria-label="停止生成" onClick={onStop} size="icon-sm" type="button">
                <SquareIcon />
              </Button>
            ) : (
              <ComposerPrimitive.Send asChild>
                <Button
                  aria-label="发送消息"
                  disabled={!canSubmit}
                  size="icon-sm"
                  type="button"
                >
                  <CornerDownLeftIcon />
                </Button>
              </ComposerPrimitive.Send>
            )}
          </div>
        </ComposerPrimitive.Root>
      </div>
    </div>
  );
}