import * as React from "react";

/**
 * 打字机效果：逐字揭示文本，配合 WS 流式 delta 形成「正在输入」观感。
 *
 * - `enabled=true`（消息 streaming 中）：按 `charsPerTick / tickMs` 速率逐字揭示，
 *   内容随 delta 增长时从当前进度继续，interval 不因文本变化重启（ref 读最新文本）。
 * - `enabled=false`（流式结束/历史消息）：立即返回全文，不做动画。
 */
export function useTypewriter(
  text: string,
  enabled: boolean,
  charsPerTick = 2,
  tickMs = 24,
  active = enabled,
): string {
  const [shown, setShown] = React.useState(0);
  const textRef = React.useRef(text);
  textRef.current = text;
  const hasPendingText = shown < text.length;

  React.useEffect(() => {
    if (!enabled || !active || !hasPendingText) return;
    const timer = setInterval(() => {
      setShown((s) => {
        const full = textRef.current.length;
        return Math.min(s + charsPerTick, full);
      });
    }, tickMs);
    return () => clearInterval(timer);
  }, [enabled, active, hasPendingText, charsPerTick, tickMs]);

  // 文本回缩（理论上不发生）时钳制显示进度。
  React.useEffect(() => {
    setShown((s) => Math.min(s, text.length));
  }, [text.length]);

  return enabled ? text.slice(0, shown) : text;
}
