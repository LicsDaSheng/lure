import { Button } from "@/components/ui/button";
import { FileCode2Icon, FileTextIcon, GitCompareArrowsIcon, LinkIcon } from "lucide-react";

import { ContentPreviewDialog } from "./content-preview-dialog";
import type { ResultDescriptor } from "./presentation";

const icons = {
  diff: GitCompareArrowsIcon,
  file: FileCode2Icon,
  log: FileTextIcon,
  link: LinkIcon,
};

export function ResultCard({ result }: { result: ResultDescriptor }) {
  const Icon = icons[result.type];
  return (
    <article className="mt-3 flex items-center gap-3 rounded-xl border bg-card p-4" aria-label={`结果：${result.title}`}>
      <div className="grid size-10 shrink-0 place-items-center rounded-lg bg-accent text-accent-foreground">
        <Icon className="size-5" />
      </div>
      <div className="min-w-0 flex-1">
        <h3 className="truncate text-sm font-medium">{result.title}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{result.description}</p>
      </div>
      <ContentPreviewDialog
        content={result.content}
        description={result.description}
        title={result.title}
        trigger={
          <Button size="sm">
            {result.actionLabel}
          </Button>
        }
      />
    </article>
  );
}
