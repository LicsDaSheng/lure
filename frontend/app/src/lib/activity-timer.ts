import { useEffect, useRef, useState } from "react";

// 模块级注册表：计时起点与已测时长按 timerKey 存活，组件卸载/重挂后仍可读
// （历史消息滚动回来仍能报出「思考了 Xs」，对齐 hermes desktop 的 activity-timer）。

const startedAtByKey = new Map<string, number>();
const durationByKey = new Map<string, number>();

function startedAt(key: string): number {
  const existing = startedAtByKey.get(key);
  if (existing !== undefined) {
    return existing;
  }

  const now = Date.now();
  startedAtByKey.set(key, now);

  return now;
}

/** 秒数格式化：<60s 显示 `Ns`，否则 `M:SS`。 */
export function formatElapsed(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  }

  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** 计时起点以来的实时秒数，`active` 期间每秒心跳一次。 */
export function useElapsedSeconds(active: boolean, timerKey?: string): number {
  const start = useRef(timerKey ? startedAt(timerKey) : Date.now());
  const lastKey = useRef(timerKey);
  const [elapsed, setElapsed] = useState(() =>
    Math.max(0, Math.floor((Date.now() - start.current) / 1000)),
  );

  if (lastKey.current !== timerKey) {
    start.current = timerKey ? startedAt(timerKey) : Date.now();
    lastKey.current = timerKey;
  }

  useEffect(() => {
    if (!active) {
      return;
    }

    if (timerKey) {
      start.current = startedAt(timerKey);
    }

    const tick = () =>
      setElapsed(Math.max(0, Math.floor((Date.now() - start.current) / 1000)));
    tick();
    const id = setInterval(tick, 1000);

    return () => clearInterval(id);
  }, [active, timerKey]);

  return elapsed;
}

/**
 * 观察某段时长直至结束并记住；从未被观察（历史加载、reasoning 以完成态到达）
 * 返回 `null`——此时无时长可报。
 */
export function useMeasuredDuration(
  active: boolean,
  timerKey: string,
): number | null {
  const elapsed = useElapsedSeconds(active, timerKey);
  const [watching, setWatching] = useState(false);
  const [measured, setMeasured] = useState<number | null>(
    () => durationByKey.get(timerKey) ?? null,
  );

  useEffect(() => {
    if (active) {
      setWatching(true);
    } else if (watching) {
      setWatching(false);
      durationByKey.set(timerKey, elapsed);
      setMeasured(elapsed);
    }
  }, [active, elapsed, timerKey, watching]);

  return measured;
}

/** 测试隔离：清空计时注册表。 */
export function __resetElapsedTimerRegistryForTests(): void {
  startedAtByKey.clear();
  durationByKey.clear();
}
