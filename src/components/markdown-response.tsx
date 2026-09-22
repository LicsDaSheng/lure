import { cjk } from "@streamdown/cjk";
import { createCodePlugin } from "@streamdown/code";
import { math } from "@streamdown/math";
import { mermaid } from "@streamdown/mermaid";
import { memo, type ComponentProps } from "react";
import { Streamdown } from "streamdown";

import { piSyntaxThemes } from "@/lib/pi-syntax-theme";
import { cn } from "@/lib/utils";

const streamdownPlugins = {
  cjk,
  code: createCodePlugin({ themes: piSyntaxThemes }),
  math,
  mermaid,
};

export const MarkdownResponse = memo(
  ({ className, lineNumbers = false, ...props }: ComponentProps<typeof Streamdown>) => (
    <Streamdown
      className={cn(
        "pi-markdown w-full min-w-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0",
        className,
      )}
      lineNumbers={lineNumbers}
      plugins={streamdownPlugins}
      {...props}
    />
  ),
  (previous, next) =>
    previous.children === next.children && previous.isAnimating === next.isAnimating,
);

MarkdownResponse.displayName = "MarkdownResponse";
