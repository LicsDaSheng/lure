import * as React from "react";
import { Pencil } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from "@/components/ui/dialog";
import {
  updateProvider,
  type ProviderRow,
  type Settings,
} from "@/lib/settings";
import { SectionTitle, Field, SaveBar, Badge, useSaver } from "./parts";

export function ProvidersPanel({
  token,
  settings,
  onChange,
}: {
  token: string;
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const [editing, setEditing] = React.useState<ProviderRow | null>(null);

  return (
    <div>
      <SectionTitle
        title="提供商"
        desc="配置各提供商的 API Key 与 API Base（覆盖默认）。密钥仅本地保存，不回显明文。"
      />

      <div className="flex flex-col gap-2">
        {settings.providers.map((p) => (
          <div
            key={p.name}
            className="flex items-center justify-between rounded-lg border bg-card px-4 py-3"
          >
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex items-center gap-2">
                <span className="font-medium">{p.label}</span>
                {p.configured ? (
                  <Badge tone="green">已配置</Badge>
                ) : (
                  <Badge>未配置</Badge>
                )}
                {!p.enabled ? <Badge>已禁用</Badge> : null}
              </div>
              <span className="truncate text-xs text-muted-foreground">
                {p.api_base ?? p.default_api_base ?? "默认 API Base"}
              </span>
            </div>
            <Button variant="outline" size="sm" onClick={() => setEditing(p)}>
              <Pencil className="size-3.5" />
              编辑
            </Button>
          </div>
        ))}
      </div>

      {editing ? (
        <ProviderDialog
          token={token}
          row={editing}
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

function ProviderDialog({
  token,
  row,
  onClose,
  onSaved,
}: {
  token: string;
  row: ProviderRow;
  onClose: () => void;
  onSaved: (s: Settings) => void;
}) {
  const [apiKey, setApiKey] = React.useState("");
  const [apiBase, setApiBase] = React.useState(row.api_base ?? "");
  const saver = useSaver();

  const save = () =>
    saver.run(async () => {
      const next = await updateProvider(token, {
        provider: row.name,
        // 未输入则不改动密钥；显式清空需用户留空并保存——此处仅在有输入时提交。
        ...(apiKey ? { api_key: apiKey } : {}),
        api_base: apiBase,
      });
      onSaved(next);
    });

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>配置 {row.label}</DialogTitle>
          <DialogDescription>
            留空 API Key 表示不修改现有值；如需清除，请提交空 API Base。
          </DialogDescription>
        </DialogHeader>

        <Field
          label="API Key"
          htmlFor="apikey"
          hint={row.configured ? "已配置（••••），留空则保持不变" : "尚未配置"}
        >
          <Input
            id="apikey"
            type="password"
            autoComplete="off"
            placeholder={row.configured ? "••••••••" : "sk-…"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </Field>

        <Field
          label="API Base"
          htmlFor="apibase"
          hint={`默认：${row.default_api_base ?? "（registry 默认）"}`}
        >
          <Input
            id="apibase"
            placeholder={row.default_api_base ?? "https://…"}
            value={apiBase}
            onChange={(e) => setApiBase(e.target.value)}
          />
        </Field>

        <div className="flex items-center justify-between">
          <SaveBar {...saver} onSave={save} />
          <DialogClose asChild>
            <Button variant="ghost">取消</Button>
          </DialogClose>
        </div>
      </DialogContent>
    </Dialog>
  );
}
