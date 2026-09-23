import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { MarqueeText } from "./marquee-text";

// jsdom 不做布局；用可控宽度模拟文字与外层可视区。
let mockScrollWidth = 0;
let mockContainerWidth = 0;
let mockNaturalWidth = 0;
const originalElementRect = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "getBoundingClientRect",
);

function stubElementWidths(
  scrollWidth: number,
  containerWidth = 0,
  naturalWidth = 0,
) {
  mockScrollWidth = scrollWidth;
  mockContainerWidth = containerWidth;
  mockNaturalWidth = naturalWidth;
  Object.defineProperty(HTMLElement.prototype, "scrollWidth", {
    configurable: true,
    get: () => mockScrollWidth,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get() {
      return this.classList.contains("flex-1") ? mockContainerWidth : mockScrollWidth;
    },
  });
  Object.defineProperty(HTMLElement.prototype, "getBoundingClientRect", {
    configurable: true,
    value() {
      return DOMRect.fromRect({
        width: this.style.width === "max-content" ? mockNaturalWidth : 0,
      });
    },
  });
}

afterEach(() => {
  delete (HTMLElement.prototype as { scrollWidth?: number }).scrollWidth;
  delete (HTMLElement.prototype as { clientWidth?: number }).clientWidth;
  if (originalElementRect) {
    Object.defineProperty(
      HTMLElement.prototype,
      "getBoundingClientRect",
      originalElementRect,
    );
  } else {
    delete (HTMLElement.prototype as { getBoundingClientRect?: () => DOMRect })
      .getBoundingClientRect;
  }
  mockScrollWidth = 0;
  mockContainerWidth = 0;
  mockNaturalWidth = 0;
});

describe("MarqueeText", () => {
  it("内容未溢出时悬停也不启动跑马灯", () => {
    stubElementWidths(80, 80);
    render(
      <div>
        <MarqueeText hovered text="短标题" />
      </div>,
    );

    const text = screen.getByText("短标题");
    expect(text).not.toHaveClass("animate-marquee");
    expect(text).toHaveClass("truncate");
  });

  it("内容溢出且悬停时以跑马灯往复滚动展示全文", () => {
    stubElementWidths(120);
    render(
      <div>
        <MarqueeText hovered text="这是一条特别长的会话标题" />
      </div>,
    );

    const text = screen.getByText("这是一条特别长的会话标题");
    expect(text).toHaveClass("animate-marquee");
    // 裁剪必须由不移动的外层容器承担；若动画元素自身也隐藏溢出，
    // 后半段文字不会被绘制，真实浏览器中平移也无法展示全文。
    expect(text).not.toHaveClass("overflow-hidden");
    expect(text.parentElement).toHaveClass("overflow-hidden");
    expect(text.parentElement).toHaveClass("w-0", "flex-1");
    // 滚动距离 = 文字 scrollWidth - 外层可视宽度。
    expect(text.style.getPropertyValue("--marquee-distance")).toBe("-120px");
    // 滚动时长按距离换算，且不低于 2 秒。
    expect(text.style.getPropertyValue("--marquee-duration")).toBe("3s");
  });

  it("内容溢出但未悬停时保持截断，不滚动", () => {
    stubElementWidths(120);
    render(
      <div>
        <MarqueeText hovered={false} text="这是一条特别长的会话标题" />
      </div>,
    );

    const text = screen.getByText("这是一条特别长的会话标题");
    expect(text).not.toHaveClass("animate-marquee");
    expect(text).toHaveClass("marquee-overflow");
    expect(text).toHaveClass("truncate");
    expect(text.style.getPropertyValue("--marquee-distance")).toBe("-120px");
  });

  it("文本变化后重新测量溢出并启动滚动", () => {
    stubElementWidths(80, 80);
    const { rerender } = render(<MarqueeText hovered text="短标题" />);

    expect(screen.getByText("短标题")).not.toHaveClass("animate-marquee");

    stubElementWidths(200);
    rerender(<MarqueeText hovered text="换成一条更长的标题" />);

    const text = screen.getByText("换成一条更长的标题");
    expect(text).toHaveClass("animate-marquee");
    expect(text.style.getPropertyValue("--marquee-distance")).toBe("-200px");
  });

  it("以外层可视宽度计算溢出，不受 WebKit 文字元素自身展开影响", () => {
    // 模拟 WebKit：被裁剪元素的 scrollWidth 与外层同为 120px，
    // 但脱离裁剪的测量副本宽度为 200px。
    stubElementWidths(120, 120, 200);
    render(<MarqueeText hovered text="这是一条 WebKit 中会展开的长标题" />);

    const text = screen.getByText("这是一条 WebKit 中会展开的长标题");
    expect(text).toHaveClass("animate-marquee");
    expect(text.style.getPropertyValue("--marquee-distance")).toBe("-80px");
    expect(text.style.getPropertyValue("--marquee-duration")).toBe("2s");
  });
});
