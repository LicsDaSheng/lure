import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MessageBubble } from "./MessageBubble";
import { __resetElapsedTimerRegistryForTests } from "@/lib/activity-timer";
import type { UiMessage } from "@/hooks/useChat";

function reasoningText(renderer: ReactTestRenderer): string {
  const box = renderer.root.findByProps({
    "data-testid": "assistant-reasoning-content",
  });
  const collect = (children: typeof box.children): string =>
    children
      .map((child) =>
        typeof child === "string" ? child : collect(child.children),
      )
      .join("");
  return collect(box.children);
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

function labelText(renderer: ReactTestRenderer): string {
  return renderer.root
    .findByProps({ "data-slot": "reasoning-label" })
    .children.filter((child) => typeof child === "string")
    .join("");
}

describe("MessageBubble 思维链", () => {
  beforeEach(() => {
    __resetElapsedTimerRegistryForTests();
    vi.useFakeTimers();
  });
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

describe("MessageBubble Hermes 消息展示", () => {
  it("用户问题使用独立整行卡片且不显示头像", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{ id: "user-1", role: "user", content: "如何读取文件？" }}
        />,
      );
    });

    const root = renderer.root.findByProps({ "data-role": "user" });
    expect(root.findByProps({ "data-slot": "user-message" })).toBeTruthy();
    expect(renderer.root.findAllByProps({ "data-slot": "message-avatar" })).toHaveLength(0);
    act(() => renderer.unmount());
  });

  it("智能体正文使用无气泡正文布局并展示完成工具", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-1",
            role: "assistant",
            content: "文件内容如下。",
            tools: [
              {
                id: "assistant-1-tool-1",
                name: "read_file",
                status: "complete",
              },
            ],
          }}
        />,
      );
    });

    expect(renderer.root.findByProps({ "data-role": "assistant" })).toBeTruthy();
    expect(renderer.root.findByProps({ "data-slot": "assistant-message" })).toBeTruthy();
    const tool = renderer.root.findByProps({ "data-slot": "tool-row" });
    expect(tool.props["data-status"]).toBe("complete");
    expect(tool.findAll((node) => node.children.includes("read_file"))).not.toHaveLength(0);
    act(() => renderer.unmount());
  });

  it("运行中的工具使用进行中状态", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-2",
            role: "assistant",
            content: "",
            streaming: true,
            tools: [
              {
                id: "assistant-2-tool-1",
                name: "web_search",
                status: "running",
              },
            ],
          }}
        />,
      );
    });

    expect(
      renderer.root.findByProps({ "data-slot": "tool-row" }).props[
        "data-status"
      ],
    ).toBe("running");
    act(() => renderer.unmount());
  });
});

describe("MessageBubble 思维链交互", () => {
  beforeEach(() => {
    __resetElapsedTimerRegistryForTests();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("流式思考显示「思考中」并带微光", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-live",
            role: "assistant",
            content: "",
            reasoning: "想",
            streaming: true,
            typewriter: true,
          }}
        />,
      );
    });

    expect(labelText(renderer)).toBe("思考中");
    expect(
      renderer.root.findByProps({ "data-slot": "reasoning-label" }).props
        .className,
    ).toContain("shimmer");
    act(() => renderer.unmount());
  });

  it("历史消息（未观察计时）显示「已思考」", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-history-2",
            role: "assistant",
            content: "回答",
            reasoning: "想",
            reasoningDone: true,
          }}
        />,
      );
    });

    expect(labelText(renderer)).toBe("已思考");
    act(() => renderer.unmount());
  });

  it("思考结束后报出时长（快速思考或思考了 Xs）", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-done",
            role: "assistant",
            content: "",
            reasoning: "想",
            streaming: true,
            typewriter: true,
          }}
        />,
      );
    });
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    act(() => {
      renderer.update(
        <MessageBubble
          message={{
            id: "assistant-done",
            role: "assistant",
            content: "答",
            reasoning: "想",
            reasoningDone: true,
            streaming: false,
            typewriter: true,
          }}
        />,
      );
    });

    expect(["快速思考", "思考了 3s"]).toContain(labelText(renderer));
    act(() => renderer.unmount());
  });

  it("reasoning 以 markdown 渲染加粗", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-md",
            role: "assistant",
            content: "",
            reasoning: "**重点** 内容",
            streaming: true,
          }}
        />,
      );
    });

    expect(renderer.root.findAllByType("strong")).not.toHaveLength(0);
    act(() => renderer.unmount());
  });

  it("空 reasoning 不渲染思考块", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(
        <MessageBubble
          message={{
            id: "assistant-empty",
            role: "assistant",
            content: "答",
            reasoning: "   ",
            reasoningDone: true,
          }}
        />,
      );
    });

    expect(
      renderer.root.findAllByProps({ "data-slot": "reasoning-toggle" }),
    ).toHaveLength(0);
    act(() => renderer.unmount());
  });
});
