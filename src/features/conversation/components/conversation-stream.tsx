import { useAppSelector } from "@/app/hooks";
import { selectConversationTurns } from "@/features/conversation";

import { AssistantTurn } from "@/features/execution";
import { MarkdownResponse } from "@/components/markdown-response";

export function ConversationStream() {
  const turns = useAppSelector(selectConversationTurns);

  return (
    <>
      {turns.map((turn) => (
        <div className="flex w-full min-w-0 flex-col gap-2" key={turn.id}>
          {turn.user && (
            <div aria-label="用户消息" className="flex w-full min-w-0 justify-end">
              <div className="flex w-full min-w-0 flex-col gap-2 overflow-hidden bg-[var(--pi-user-bg)] px-4 py-2 text-sm text-foreground">
                <MarkdownResponse>
                  {turn.user.parts.flatMap((part) => (part.type === "text" ? [part.text] : [])).join("")}
                </MarkdownResponse>
              </div>
            </div>
          )}
          <AssistantTurn turn={turn} />
        </div>
      ))}
    </>
  );
}
