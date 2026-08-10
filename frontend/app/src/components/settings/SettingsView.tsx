import * as React from "react";
import { X, SlidersHorizontal, Boxes, KeyRound, Info, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  fetchSettings,
  type Settings,
} from "@/lib/settings";
import { GeneralPanel } from "./GeneralPanel";
import { PresetsPanel } from "./PresetsPanel";
import { ProvidersPanel } from "./ProvidersPanel";
import { AboutPanel } from "./AboutPanel";

const TABS = [
  { value: "general", label: "通用", icon: SlidersHorizontal },
  { value: "presets", label: "模型预设", icon: Boxes },
  { value: "providers", label: "提供商", icon: KeyRound },
  { value: "about", label: "关于", icon: Info },
] as const;

export function SettingsView({
  token,
  onClose,
}: {
  token: string;
  onClose: () => void;
}) {
  const [settings, setSettings] = React.useState<Settings | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let disposed = false;
    fetchSettings(token)
      .then((s) => !disposed && setSettings(s))
      .catch((e) => !disposed && setError(String(e.message ?? e)));
    return () => {
      disposed = true;
    };
  }, [token]);

  // Esc 关闭。
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-40 flex flex-col bg-background">
      <header className="flex h-14 items-center justify-between border-b px-5">
        <h1 className="text-base font-semibold tracking-tight">设置</h1>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="关闭设置">
          <X className="size-4" />
        </Button>
      </header>

      {error ? (
        <div className="m-6 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-3 text-sm text-red-500">
          加载设置失败：{error}
        </div>
      ) : !settings ? (
        <div className="flex flex-1 items-center justify-center text-muted-foreground">
          <Loader2 className="mr-2 size-4 animate-spin" />
          加载中…
        </div>
      ) : (
        <Tabs
          defaultValue="general"
          orientation="vertical"
          className="flex min-h-0 flex-1"
        >
          <TabsList className="w-[200px] shrink-0 border-r bg-sidebar p-3">
            {TABS.map(({ value, label, icon: Icon }) => (
              <TabsTrigger key={value} value={value}>
                <Icon className="size-4" />
                {label}
              </TabsTrigger>
            ))}
          </TabsList>

          <ScrollArea className="flex-1">
            <div className="mx-auto max-w-2xl px-8 py-8">
              <TabsContent value="general">
                <GeneralPanel token={token} settings={settings} onChange={setSettings} />
              </TabsContent>
              <TabsContent value="presets">
                <PresetsPanel token={token} settings={settings} onChange={setSettings} />
              </TabsContent>
              <TabsContent value="providers">
                <ProvidersPanel token={token} settings={settings} onChange={setSettings} />
              </TabsContent>
              <TabsContent value="about">
                <AboutPanel settings={settings} />
              </TabsContent>
            </div>
          </ScrollArea>
        </Tabs>
      )}
    </div>
  );
}
