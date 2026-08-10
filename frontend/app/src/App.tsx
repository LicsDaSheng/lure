import * as React from "react";
import { Sidebar } from "@/components/Sidebar";
import { Thread } from "@/components/Thread";
import { Composer } from "@/components/Composer";
import { SettingsView } from "@/components/settings/SettingsView";
import { useChat } from "@/hooks/useChat";
import { cn } from "@/lib/utils";

export default function App() {
  const {
    conn,
    apiToken,
    modelName,
    sessions,
    activeKey,
    messages,
    streaming,
    selectSession,
    newChat,
    send,
  } = useChat();
  const [settingsOpen, setSettingsOpen] = React.useState(false);

  return (
    <div className="flex h-full">
      <Sidebar
        sessions={sessions}
        activeKey={activeKey}
        onSelect={selectSession}
        onNew={newChat}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 items-center justify-between border-b px-5">
          <div className="flex items-center gap-2 text-sm font-medium">
            {modelName ?? "Lure"}
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span
              className={cn(
                "size-2 rounded-full",
                conn === "ready"
                  ? "bg-emerald-500"
                  : conn === "connecting"
                    ? "bg-amber-500"
                    : "bg-red-500",
              )}
            />
            {conn === "ready"
              ? "已连接"
              : conn === "connecting"
                ? "连接中"
                : "连接失败"}
          </div>
        </header>
        <Thread messages={messages} modelName={modelName} />
        <Composer disabled={streaming || conn !== "ready"} onSend={send} />
      </main>

      {settingsOpen && apiToken ? (
        <SettingsView token={apiToken} onClose={() => setSettingsOpen(false)} />
      ) : null}
    </div>
  );
}
