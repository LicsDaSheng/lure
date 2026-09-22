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
    <div className="flex min-h-0 flex-1 translate-y-16 flex-col items-center justify-center px-6 text-center">
      <div className="grid size-12 place-items-center rounded-xl bg-[#F7F7F8] text-[#3C3C40]">
        <BotIcon className="size-5" />
      </div>
      <h2 className="mt-6 text-xl font-semibold tracking-tight">开始一个新任务</h2>
      <p className="mt-5 max-w-lg text-sm leading-6 text-muted-foreground">
        {projectName
          ? `Pi 会在 ${projectName} 中工作，描述你想完成的事情。`
          : "正在准备默认工作目录，然后描述你想完成的事情。"}
      </p>
      <div className="mt-8 flex flex-wrap justify-center gap-2">
        {workflowExamples.map((example) => (
          <Button
            key={example}
            onClick={() => onPickExample(example)}
            className="h-8 rounded-lg bg-[#F4F4F6] px-3 text-xs hover:bg-[#E9E9ED]"
            size="sm"
            type="button"
            variant="secondary"
          >
            {example}
          </Button>
        ))}
      </div>
    </div>
  );
}