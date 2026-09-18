import {
  Conversation,
  ConversationContent,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import {
  PromptInput,
  PromptInputBody,
  PromptInputFooter,
  type PromptInputMessage,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
} from "@/components/ai-elements/prompt-input";
import { Badge } from "@/components/ui/badge";
import { TooltipProvider } from "@/components/ui/tooltip";
import { BotIcon, FolderIcon, PlusIcon } from "lucide-react";
import { useCallback, useState } from "react";

type ConversationMessage = {
  id: string;
  role: "assistant" | "user";
  content: string;
};

const initialMessages: ConversationMessage[] = [
  {
    id: "welcome",
    role: "assistant",
    content:
      "欢迎使用 **Lure**。选择工作目录并连接 Pi 后，即可在这里开始对话。",
  },
];

function App() {
  const [messages, setMessages] =
    useState<ConversationMessage[]>(initialMessages);

  const handleSubmit = useCallback((message: PromptInputMessage) => {
    const content = message.text.trim();
    if (!content) {
      return;
    }

    setMessages((current) => [
      ...current,
      {
        id: crypto.randomUUID(),
        role: "user",
        content,
      },
    ]);
  }, []);

  return (
    <TooltipProvider>
      <main className="flex h-screen min-h-0 bg-background text-foreground">
        <aside className="flex w-64 shrink-0 flex-col border-r bg-card/45 p-3">
          <div className="flex items-center gap-2 px-2 py-3">
            <div className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground">
              <BotIcon className="size-4" />
            </div>
            <div>
              <h1 className="font-semibold tracking-tight">Lure</h1>
              <p className="text-xs text-muted-foreground">Pi 桌面客户端</p>
            </div>
          </div>

          <button
            className="mt-3 flex items-center gap-2 rounded-lg border bg-background px-3 py-2 text-sm transition-colors hover:bg-accent"
            type="button"
          >
            <PlusIcon className="size-4" />
            新建会话
          </button>

          <div className="mt-auto rounded-lg border border-dashed p-3 text-xs text-muted-foreground">
            <div className="mb-1 flex items-center gap-2 font-medium text-foreground">
              <FolderIcon className="size-3.5" />
              工作目录
            </div>
            尚未选择
          </div>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-14 shrink-0 items-center justify-between border-b px-5">
            <div>
              <p className="text-sm font-medium">新会话</p>
              <p className="text-xs text-muted-foreground">等待连接 Pi RPC</p>
            </div>
            <Badge variant="outline" className="gap-1.5">
              <span className="size-1.5 rounded-full bg-amber-400" />
              未连接
            </Badge>
          </header>

          <Conversation className="min-h-0">
            <ConversationContent className="mx-auto w-full max-w-3xl px-6 py-8">
              {messages.map((message) => (
                <Message from={message.role} key={message.id}>
                  <MessageContent>
                    <MessageResponse>{message.content}</MessageResponse>
                  </MessageContent>
                </Message>
              ))}
            </ConversationContent>
            <ConversationScrollButton />
          </Conversation>

          <div className="shrink-0 border-t bg-background/90 px-6 py-4 backdrop-blur">
            <PromptInput
              className="mx-auto w-full max-w-3xl rounded-2xl"
              onSubmit={handleSubmit}
            >
              <PromptInputBody>
                <PromptInputTextarea placeholder="给 Pi 发送消息…" />
              </PromptInputBody>
              <PromptInputFooter>
                <PromptInputTools>
                  <span className="px-2 text-xs text-muted-foreground">
                    Enter 发送 · Shift+Enter 换行
                  </span>
                </PromptInputTools>
                <PromptInputSubmit aria-label="发送消息" status="ready" />
              </PromptInputFooter>
            </PromptInput>
          </div>
        </section>
      </main>
    </TooltipProvider>
  );
}

export default App;
