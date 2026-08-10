import * as React from "react";
import { Pencil, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from "@/components/ui/dialog";
import {
  createPreset,
  updatePreset,
  type ModelPreset,
  type Settings,
} from "@/lib/settings";
import { SectionTitle, Field, SaveBar, Badge, useSaver } from "./parts";

type Editing =
  | { mode: "create" }
  | { mode: "edit"; preset: ModelPreset }
  | null;

export function PresetsPanel({
  token,
  settings,
  onChange,
}: {
  token: string;
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const [editing, setEditing] = React.useState<Editing>(null);

  return (
    <div>
      <SectionTitle
        title="模型预设"
        desc="命名的模型参数组合。默认预设对应通用页的 agent 默认，此处不可编辑。"
      />

      <div className="mb-4 flex flex-col gap-2">
        {settings.model_presets.map((p) => (
          <div
            key={p.name}
            className="flex items-center justify-between rounded-lg border bg-card px-4 py-3"
          >
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex items-center gap-2">
                <span className="font-medium">{p.label}</span>
                {p.is_default ? <Badge tone="accent">默认</Badge> : null}
                {p.active ? <Badge tone="green">生效中</Badge> : null}
              </div>
              <span className="truncate text-xs text-muted-foreground">
                {p.provider} · {p.model} · ctx {p.context_window_tokens}
              </span>
            </div>
            {!p.is_default ? (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setEditing({ mode: "edit", preset: p })}
              >
                <Pencil className="size-3.5" />
                编辑
              </Button>
            ) : null}
          </div>
        ))}
      </div>

      <Button variant="outline" onClick={() => setEditing({ mode: "create" })}>
        <Plus className="size-4" />
        新建预设
      </Button>

      {editing ? (
        <PresetDialog
          token={token}
          providers={settings.providers.map((p) => p.name)}
          editing={editing}
          onClose={() => setEditing(null)}
          onSaved={(s) => {
            onChange(s);
            setEditing(null);
          }}
        />
      ) : null}
    </div>
  );
}

function PresetDialog({
  token,
  providers,
  editing,
  onClose,
  onSaved,
}: {
  token: string;
  providers: string[];
  editing: Exclude<Editing, null>;
  onClose: () => void;
  onSaved: (s: Settings) => void;
}) {
  const isEdit = editing.mode === "edit";
  const base = isEdit ? editing.preset : null;
  const [name, setName] = React.useState(base?.name ?? "");
  const [label, setLabel] = React.useState(base?.label ?? "");
  const [provider, setProvider] = React.useState(
    base?.provider ?? providers[0] ?? "auto",
  );
  const [model, setModel] = React.useState(base?.model ?? "");
  const [ctx, setCtx] = React.useState(
    base ? String(base.context_window_tokens) : "",
  );
  const saver = useSaver();

  const save = () =>
    saver.run(async () => {
      const next = isEdit
        ? await updatePreset(token, {
            name,
            label,
            provider,
            model,
            context_window_tokens: ctx ? Number(ctx) : undefined,
          })
        : await createPreset(token, { name, model, label, provider });
      onSaved(next);
    });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{isEdit ? `编辑预设 ${name}` : "新建预设"}</DialogTitle>
          <DialogDescription>
            {isEdit
              ? "修改该命名预设的模型参数。"
              : "创建一个命名预设；名称不可为 default。"}
          </DialogDescription>
        </DialogHeader>

        {!isEdit ? (
          <Field label="名称" htmlFor="pname" hint="唯一标识，如 fast">
            <Input id="pname" value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
        ) : null}

        <Field label="显示名" htmlFor="plabel">
          <Input id="plabel" value={label} onChange={(e) => setLabel(e.target.value)} />
        </Field>

        <Field label="提供商">
          <Select value={provider} onValueChange={setProvider}>
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {providers.map((p) => (
                <SelectItem key={p} value={p}>
                  {p}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>

        <Field label="模型" htmlFor="pmodel">
          <Input id="pmodel" value={model} onChange={(e) => setModel(e.target.value)} />
        </Field>

        {isEdit ? (
          <Field label="上下文窗口 (tokens)" htmlFor="pctx">
            <Input
              id="pctx"
              type="number"
              min={0}
              value={ctx}
              onChange={(e) => setCtx(e.target.value)}
            />
          </Field>
        ) : null}

        <div className="flex items-center justify-between">
          <SaveBar {...saver} onSave={save} label={isEdit ? "保存" : "创建"} />
          <DialogClose asChild>
            <Button variant="ghost">取消</Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
