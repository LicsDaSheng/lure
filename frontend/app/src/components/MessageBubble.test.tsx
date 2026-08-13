import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MessageBubble } from "./MessageBubble";
import type { UiMessage } from "@/hooks/useChat";

function reasoningText(renderer: ReactTestRenderer): string {
  return renderer.root
    .findByProps({ "data-testid": "assistant-reasoning-content" })
    .children.filter((child) => typeof child === "string")
    .join("");
}

function contentText(renderer: ReactTestRenderer): string {
  const prose = renderer.root
    .findByProps({ "data-testid": "assistant-message-content" })
    .findByProps({ className: "prose-chat" });
  const collect = (children: typeof prose.children): string =>
    children
      .map((child) =>
        typeof child === "string" ? child : collect(child.children),
      )
      .join("");
  return collect(prose.children);
}

describe("MessageBubble 思维链", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("新回答自动展开思维链，并在 reasoning delta 增长时继续逐字揭示", () => {
    const message: UiMessage = {
      id: "assistant-1",
      role: "assistant",
      content: "",
      reasoning: "思考中",
      streaming: true,
      typewriter: true,
    };
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(<MessageBubble message={message} />);
    });
    act(() => {
      vi.advanceTimersByTime(24);
    });
    expect(reasoningText(renderer)).toBe("思考");

    act(() => {
      renderer.update(
        <MessageBubble message={{ ...message, reasoning: "思考中的新片段" }} />,
      );
    });
    act(() => {
      vi.advanceTimersByTime(72);
    });

    expect(reasoningText(renderer)).toBe("思考中的新片段");
    act(() => renderer.unmount());
  });

  it("历史消息的思维链保持折叠且不播放动画", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-history",
            role: "assistant",
            content: "回答",
            reasoning: "历史思维链",
          }}
        />,
      );
    });

    expect(
      renderer.root.findAllByProps({
        "data-testid": "assistant-reasoning-content",
      }),
    ).toHaveLength(0);
    act(() => renderer.unmount());
  });

  it("思维链存在显示积压时，正文必须等待思维链揭示完成", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-sequence",
            role: "assistant",
            reasoning: "思考过程",
            content: "答案",
            streaming: true,
            typewriter: true,
          }}
        />,
      );
    });

    act(() => {
      vi.advanceTimersByTime(24);
    });
    expect(reasoningText(renderer)).toBe("思考");
    expect(contentText(renderer)).toBe("");

    act(() => {
      vi.advanceTimersByTime(24);
    });
    expect(reasoningText(renderer)).toBe("思考过程");
    expect(contentText(renderer)).toBe("");

    act(() => {
      vi.advanceTimersByTime(24);
    });
    expect(contentText(renderer)).toBe("答案");
    act(() => renderer.unmount());
  });
});
