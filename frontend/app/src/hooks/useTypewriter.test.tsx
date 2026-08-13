import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTypewriter } from "./useTypewriter";

function Probe({ text, enabled }: { text: string; enabled: boolean }) {
  const shown = useTypewriter(text, enabled, 1, 20);
  return <span>{shown}</span>;
}

function renderedText(renderer: ReactTestRenderer): string {
  return renderer.root.findByType("span").children.join("");
}

describe("useTypewriter", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("追上当前 delta 后，新 delta 到达会继续揭示", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(<Probe text="a" enabled />);
    });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    expect(renderedText(renderer)).toBe("a");

    act(() => {
      renderer.update(<Probe text="abc" enabled />);
    });
    act(() => {
      vi.advanceTimersByTime(40);
    });

    expect(renderedText(renderer)).toBe("abc");
    act(() => renderer.unmount());
  });

  it("reasoning 阶段正文为空时暂停，首个正文 delta 到达后开始揭示", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(<Probe text="" enabled />);
    });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    act(() => {
      renderer.update(<Probe text="答案" enabled />);
    });
    act(() => {
      vi.advanceTimersByTime(40);
    });

    expect(renderedText(renderer)).toBe("答案");
    act(() => renderer.unmount());
  });

  it("禁用动画时，后续文本增长立即显示全文", () => {
    let renderer!: ReactTestRenderer;
    act(() => {
      renderer = create(<Probe text="" enabled={false} />);
    });
    act(() => {
      renderer.update(<Probe text="完整回答" enabled={false} />);
    });

    expect(renderedText(renderer)).toBe("完整回答");
    act(() => renderer.unmount());
  });
});
