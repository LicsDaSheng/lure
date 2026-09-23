import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";

import { cn } from "@/lib/utils";

const MARQUEE_SPEED_PX_PER_S = 40;
const MARQUEE_MIN_DURATION_S = 2;

/**
 * 在固定宽度内展示长文本：常态截断显示省略号；当内容溢出且处于悬停态时，
 * 以跑马灯（往复滚动）在原位展示完整文本。悬停结束回到截断态。
 *
 * 溢出距离与时长在测量后写入 CSS 变量，供 `animate-marquee` 关键帧使用；
 * 系统开启“减弱动态效果”时不滚动（`motion-reduce`）。
 */
export function MarqueeText({
  text,
  hovered = false,
  className,
}: {
  text: string;
  hovered?: boolean;
  className?: string;
}) {
  const containerRef = useRef<HTMLSpanElement>(null);
  const textRef = useRef<HTMLSpanElement>(null);
  const [overflowPx, setOverflowPx] = useState(0);
  const [rowActive, setRowActive] = useState(false);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const element = textRef.current;
    if (!container || !element) return;
    const measure = () => {
      // WebKit 可能将带省略号裁剪的元素 scrollWidth 报为可视宽度。
      // 用脱离布局且不裁剪的短暂副本测量真实自然宽度，测完立即移除。
      const measuringElement = element.cloneNode(true) as HTMLSpanElement;
      Object.assign(measuringElement.style, {
        animation: "none",
        inset: "0 auto auto 0",
        maxWidth: "none",
        overflow: "visible",
        pointerEvents: "none",
        position: "fixed",
        textOverflow: "clip",
        transform: "none",
        visibility: "hidden",
        whiteSpace: "nowrap",
        width: "max-content",
      });
      document.body.append(measuringElement);
      const naturalWidth = measuringElement.getBoundingClientRect().width;
      measuringElement.remove();
      const contentWidth = Math.max(element.scrollWidth, naturalWidth);
      setOverflowPx(Math.max(0, contentWidth - container.clientWidth));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    observer.observe(element);
    return () => observer.disconnect();
  }, [text]);

  useEffect(() => {
    const container = containerRef.current;
    const row = container?.closest<HTMLElement>(".marquee-row");
    if (!row) return;

    const activate = () => setRowActive(true);
    const deactivate = () => {
      if (!row.matches(":hover") && !row.contains(document.activeElement)) {
        setRowActive(false);
      }
    };
    row.addEventListener("pointerenter", activate);
    row.addEventListener("pointerleave", deactivate);
    row.addEventListener("focusin", activate);
    row.addEventListener("focusout", deactivate);
    return () => {
      row.removeEventListener("pointerenter", activate);
      row.removeEventListener("pointerleave", deactivate);
      row.removeEventListener("focusin", activate);
      row.removeEventListener("focusout", deactivate);
    };
  }, []);

  const marqueeActive = (hovered || rowActive) && overflowPx > 0;
  const hasOverflow = overflowPx > 0;

  useEffect(() => {
    const element = textRef.current;
    if (
      !element ||
      !marqueeActive ||
      typeof element.animate !== "function" ||
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      return;
    }

    const durationMs =
      Math.max(MARQUEE_MIN_DURATION_S, overflowPx / MARQUEE_SPEED_PX_PER_S) *
      1000;
    const animation = element.animate(
      [
        { transform: "translateX(0)" },
        { transform: `translateX(${-overflowPx}px)` },
      ],
      {
        direction: "alternate",
        duration: durationMs,
        easing: "ease-in-out",
        iterations: Infinity,
      },
    );
    return () => animation.cancel();
  }, [marqueeActive, overflowPx]);

  return (
    <span
      className={cn("w-0 min-w-0 flex-1 overflow-hidden", className)}
      ref={containerRef}
    >
      <span
        className={
          marqueeActive
            ? "marquee-overflow block whitespace-nowrap animate-marquee motion-reduce:animate-none"
            : hasOverflow
              ? "marquee-overflow block truncate"
              : "block truncate"
        }
        ref={textRef}
        style={
          hasOverflow
            ? ({
                "--marquee-distance": `${-overflowPx}px`,
                "--marquee-duration": `${Math.max(
                  MARQUEE_MIN_DURATION_S,
                  overflowPx / MARQUEE_SPEED_PX_PER_S,
                )}s`,
              } as CSSProperties)
            : undefined
        }
      >
        {text}
      </span>
    </span>
  );
}
