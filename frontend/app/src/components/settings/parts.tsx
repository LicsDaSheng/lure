import * as React from "react";
import { Check, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

export function SectionTitle({
  title,
  desc,
}: {
  title: string;
  desc?: string;
}) {
  return (
    <div className="mb-5">
      <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
      {desc ? <p className="mt-1 text-sm text-muted-foreground">{desc}</p> : null}
    </div>
  );
}

export function Field({
  label,
  hint,
  htmlFor,
  children,
}: {
  label: string;
  hint?: string;
  htmlFor?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-4 flex flex-col gap-1.5">
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
    </div>
  );
}

/** 封装保存动作：pending/成功闪现/错误三态，返回触发器与状态节点。 */
export function useSaver() {
  const [saving, setSaving] = React.useState(false);
  const [saved, setSaved] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const run = React.useCallback(async (fn: () => Promise<void>) => {
    setSaving(true);
    setError(null);
    try {
      await fn();
      setSaved(true);
      setTimeout(() => setSaved(false), 1800);
    } catch (e) {
      setError(String((e as Error).message ?? e));
    } finally {
      setSaving(false);
    }
  }, []);

  return { saving, saved, error, run };
}

export function SaveBar({
  saving,
  saved,
  error,
  onSave,
  label = "保存",
}: {
  saving: boolean;
  saved: boolean;
  error: string | null;
  onSave: () => void;
  label?: string;
}) {
  return (
    <div className="mt-2 flex items-center gap-3">
      <Button onClick={onSave} disabled={saving}>
        {saving ? <Loader2 className="size-4 animate-spin" /> : null}
        {label}
      </Button>
      {saved ? (
        <span className="flex items-center gap-1 text-sm text-emerald-500">
          <Check className="size-4" /> 已保存
        </span>
      ) : null}
      {error ? <span className="text-sm text-red-500">{error}</span> : null}
    </div>
  );
}

export function Badge({
  children,
  tone = "muted",
}: {
  children: React.ReactNode;
  tone?: "muted" | "accent" | "green";
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
        tone === "accent" && "bg-accent/15 text-accent",
        tone === "green" && "bg-emerald-500/15 text-emerald-500",
        tone === "muted" && "bg-muted text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}
