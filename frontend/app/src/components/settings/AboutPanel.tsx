import type { Settings } from "@/lib/settings";
import { SectionTitle } from "./parts";

export function AboutPanel({ settings }: { settings: Settings }) {
  const rows = [
    { label: "版本", value: settings.version.current },
    { label: "Workspace", value: settings.runtime.workspace_path || "（默认）" },
  ];
  return (
    <div>
      <SectionTitle title="关于" desc="运行时信息。" />
      <dl className="flex flex-col gap-3">
        {rows.map((r) => (
          <div
            key={r.label}
            className="flex items-center justify-between rounded-lg border bg-card px-4 py-3"
          >
            <dt className="text-sm text-muted-foreground">{r.label}</dt>
            <dd className="max-w-[60%] truncate font-mono text-sm">{r.value}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
