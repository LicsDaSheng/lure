import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { ConnectionPhase, ModelSnapshot } from "@/lib/pi-rpc/types";
import { cn } from "@/lib/utils";

const thinkingLevels = ["off", "minimal", "low", "medium", "high"] as const;

export function ModelControls({
  model,
  models,
  onSelectModel,
  onSelectThinkingLevel,
  phase,
  thinkingLevel,
}: {
  model: ModelSnapshot | null;
  models: ModelSnapshot[];
  onSelectModel: (provider: string, modelId: string) => void;
  onSelectThinkingLevel: (level: string) => void;
  phase: ConnectionPhase;
  thinkingLevel: string | null;
}) {
  const [open, setOpen] = useState(false);
  const [activeProvider, setActiveProvider] = useState("");
  const [pendingModelKey, setPendingModelKey] = useState("");
  const modelOptions = useMemo(() => models.length ? models : model ? [model] : [], [model, models]);
  const modelGroups = useMemo(() => {
    const groups = new Map<string, ModelSnapshot[]>();
    for (const option of modelOptions) {
      const group = groups.get(option.provider) ?? [];
      group.push(option);
      groups.set(option.provider, group);
    }
    return groups;
  }, [modelOptions]);
  const modelKey = model ? `${model.provider}::${model.id}` : "";
  const selectedModelKey = modelOptions.some((option) => `${option.provider}::${option.id}` === modelKey)
    ? modelKey
    : modelOptions[0] ? `${modelOptions[0].provider}::${modelOptions[0].id}` : "";

  const openPicker = useCallback(() => {
    const selected = modelOptions.find((option) => `${option.provider}::${option.id}` === selectedModelKey) ?? modelOptions[0];
    if (!selected) return;
    setActiveProvider(selected.provider);
    setPendingModelKey(`${selected.provider}::${selected.id}`);
    setOpen(true);
  }, [modelOptions, selectedModelKey]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.repeat || !event.ctrlKey || event.altKey || event.metaKey || event.key.toLowerCase() !== "l" || open || modelOptions.length === 0) return;
      event.preventDefault();
      openPicker();
    };
    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [modelOptions.length, open, openPicker]);

  const confirmSelection = () => {
    const selected = modelOptions.find((option) => `${option.provider}::${option.id}` === pendingModelKey);
    if (!selected) return;
    setOpen(false);
    onSelectModel(selected.provider, selected.id);
  };

  return (
    <>
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
                <option key={`${option.provider}::${option.id}`} value={`${option.provider}::${option.id}`}>
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
        {thinkingLevels.map((level) => <option key={level} value={level}>{level}</option>)}
      </select>

      <Dialog open={open} onOpenChange={setOpen}>
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
                className={cn("shrink-0 border-b-2 px-3 py-2 text-sm text-muted-foreground", provider === activeProvider ? "border-primary text-foreground" : "border-transparent hover:text-foreground")}
                id={`model-provider-tab-${provider}`}
                key={provider}
                onClick={() => {
                  setActiveProvider(provider);
                  if (!options.some((option) => `${option.provider}::${option.id}` === pendingModelKey)) {
                    setPendingModelKey(`${options[0]?.provider}::${options[0]?.id}`);
                  }
                }}
                role="tab"
                type="button"
              >{provider}</button>
            ))}
          </div>
          {[...modelGroups].map(([provider, options]) => provider === activeProvider ? (
            <div aria-labelledby={`model-provider-tab-${provider}`} className="max-h-72 space-y-2 overflow-y-auto" id={`model-provider-${provider}`} key={provider} role="tabpanel">
              <div aria-label="可用模型" className="grid gap-2" role="radiogroup">
                {options.map((option) => {
                  const optionKey = `${option.provider}::${option.id}`;
                  const selected = optionKey === pendingModelKey;
                  return (
                    <button
                      aria-checked={selected}
                      className={cn("rounded-lg border px-3 py-2 text-left text-sm", selected ? "border-primary bg-accent text-foreground" : "border-border hover:bg-accent/50")}
                      key={optionKey}
                      onClick={() => setPendingModelKey(optionKey)}
                      role="radio"
                      type="button"
                    >{option.id}</button>
                  );
                })}
              </div>
            </div>
          ) : null)}
          <DialogFooter>
            <Button onClick={() => setOpen(false)} type="button" variant="outline">取消</Button>
            <Button onClick={confirmSelection} type="button">确认选择</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
