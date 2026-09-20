import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Maximize2Icon } from "lucide-react";
import type { ReactNode } from "react";

export function ContentPreviewDialog({
  title,
  description,
  content,
  trigger,
}: {
  title: string;
  description: string;
  content: string;
  trigger?: ReactNode;
}) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        {trigger ?? (
          <Button size="sm" variant="outline">
            <Maximize2Icon />
            查看完整内容
          </Button>
        )}
      </DialogTrigger>
      <DialogContent className="h-dvh max-h-dvh max-w-none rounded-none p-0 sm:h-[80vh] sm:max-h-[80vh] sm:w-[min(880px,calc(100vw-3rem))] sm:max-w-none sm:rounded-xl">
        <DialogHeader className="border-b px-5 py-4 pr-12">
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <pre className="min-h-0 overflow-auto whitespace-pre-wrap break-words px-5 pb-6 font-mono text-xs leading-5">
          {content || "没有可显示的内容"}
        </pre>
      </DialogContent>
    </Dialog>
  );
}
