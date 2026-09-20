import { Button } from "@/components/ui/button";
import { BotIcon } from "lucide-react";

export const workflowExamples = [
  "分析当前项目",
  "检查未提交改动",
  "解释代码结构",
] as const;

export function EmptyState({
  projectName,
  onPickExample,
}: {
  projectName: string | null;
  onPickExample: (example: string) => void;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 px-6 pb-[6vh] pt-[12vh] text-center">
      <div className="grid size-12 place-items-center rounded-xl bg-accent text-accent-foreground">
        <BotIcon className="size-6" />
      </div>
      <h2 className="text-xl font-semibold tracking-tight">开始一个新任务</h2>
      <p className="max-w-lg text-sm leading-6 text-muted-foreground">
        {projectName
          ? `Pi 会在 ${projectName} 中工作，描述你想完成的事情。`
          : "选择工作目录并连接 Pi，然后描述你想完成的事情。"}
      </p>
      <div className="mt-1 flex flex-wrap justify-center gap-2">
        {workflowExamples.map((example) => (
          <Button
            key={example}
            onClick={() => onPickExample(example)}
            size="sm"
            type="button"
            variant="ghost"
          >
            {example}
          </Button>
        ))}
      </div>
    </div>
  );
}