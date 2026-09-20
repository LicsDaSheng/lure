"use client";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { UIMessage } from "ai";
import { ArrowDownIcon, DownloadIcon } from "lucide-react";
import type { ComponentProps, HTMLAttributes, ReactNode } from "react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

const FOLLOW_THRESHOLD = 24;

type ConversationContextValue = {
  isAtBottom: boolean;
  contentRef: (node: HTMLDivElement | null) => void;
  scrollToBottom: () => void;
};

const ConversationContext = createContext<ConversationContextValue | null>(null);

export type ConversationProps = HTMLAttributes<HTMLDivElement>;

export const Conversation = ({ className, children, onScroll, ...props }: ConversationProps) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const contentElementRef = useRef<HTMLDivElement | null>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const followingRef = useRef(true);

  const measure = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
    const atBottom = distance <= FOLLOW_THRESHOLD;
    followingRef.current = atBottom;
    setIsAtBottom(atBottom);
  }, []);

  const scrollToBottom = useCallback(() => {
    const element = scrollRef.current;
    if (!element) return;
    element.scrollTo({ top: element.scrollHeight, behavior: "auto" });
    followingRef.current = true;
    setIsAtBottom(true);
  }, []);

  const contentRef = useCallback((node: HTMLDivElement | null) => {
    contentElementRef.current = node;
  }, []);

  useEffect(() => {
    const content = contentElementRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (followingRef.current) scrollToBottom();
      else measure();
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [measure, scrollToBottom]);

  const context = useMemo(
    () => ({ contentRef, isAtBottom, scrollToBottom }),
    [contentRef, isAtBottom, scrollToBottom],
  );

  return (
    <ConversationContext.Provider value={context}>
      <div
        className={cn("relative flex-1 overflow-y-auto", className)}
        onScroll={(event) => {
          measure();
          onScroll?.(event);
        }}
        ref={scrollRef}
        role="log"
        {...props}
      >
        {children}
      </div>
    </ConversationContext.Provider>
  );
};

export type ConversationContentProps = HTMLAttributes<HTMLDivElement>;

export const ConversationContent = ({ className, ...props }: ConversationContentProps) => {
  const context = useContext(ConversationContext);
  return (
    <div
      className={cn("flex flex-col gap-2 p-4", className)}
      ref={context?.contentRef}
      {...props}
    />
  );
};

export type ConversationEmptyStateProps = ComponentProps<"div"> & {
  title?: string;
  description?: string;
  icon?: ReactNode;
};

export const ConversationEmptyState = ({
  className,
  title = "No messages yet",
  description = "Start a conversation to see messages here",
  icon,
  children,
  ...props
}: ConversationEmptyStateProps) => (
  <div className={cn("flex size-full flex-col items-center justify-center gap-3 p-8 text-center", className)} {...props}>
    {children ?? (
      <>
        {icon && <div className="text-muted-foreground">{icon}</div>}
        <div className="space-y-1">
          <h3 className="text-sm font-medium">{title}</h3>
          {description && <p className="text-sm text-muted-foreground">{description}</p>}
        </div>
      </>
    )}
  </div>
);

export type ConversationScrollButtonProps = ComponentProps<typeof Button>;

export const ConversationScrollButton = ({ className, ...props }: ConversationScrollButtonProps) => {
  const context = useContext(ConversationContext);
  if (!context || context.isAtBottom) return null;
  return (
    <Button
      className={cn("sticky bottom-4 left-1/2 -translate-x-1/2 rounded-none shadow-none", className)}
      onClick={context.scrollToBottom}
      size="icon"
      type="button"
      variant="outline"
      {...props}
    >
      <ArrowDownIcon className="size-4" />
    </Button>
  );
};

const getMessageText = (message: UIMessage): string =>
  message.parts.filter((part) => part.type === "text").map((part) => part.text).join("");

export type ConversationDownloadProps = Omit<ComponentProps<typeof Button>, "onClick"> & {
  messages: UIMessage[];
  filename?: string;
  formatMessage?: (message: UIMessage, index: number) => string;
};

const defaultFormatMessage = (message: UIMessage): string => {
  const roleLabel = message.role.charAt(0).toUpperCase() + message.role.slice(1);
  return `**${roleLabel}:** ${getMessageText(message)}`;
};

export const messagesToMarkdown = (
  messages: UIMessage[],
  formatMessage: (message: UIMessage, index: number) => string = defaultFormatMessage,
): string => messages.map((message, index) => formatMessage(message, index)).join("\n\n");

export const ConversationDownload = ({
  messages,
  filename = "conversation.md",
  formatMessage = defaultFormatMessage,
  className,
  children,
  ...props
}: ConversationDownloadProps) => {
  const handleDownload = useCallback(() => {
    const markdown = messagesToMarkdown(messages, formatMessage);
    const blob = new Blob([markdown], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    document.body.append(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }, [messages, filename, formatMessage]);

  return (
    <Button className={cn("absolute top-4 right-4 rounded-none shadow-none", className)} onClick={handleDownload} size="icon" type="button" variant="outline" {...props}>
      {children ?? <DownloadIcon className="size-4" />}
    </Button>
  );
};
