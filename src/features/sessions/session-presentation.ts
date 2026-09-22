import type { PiSessionSummary } from "@/lib/pi-rpc/types";

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

export function sessionTitle(session: PiSessionSummary, fallback: string): string {
  const name = session.name?.trim();
  if (name) return name;
  const firstLine = session.firstMessage?.split("\n")[0]?.trim();
  if (firstLine) return firstLine.length > 60 ? `${firstLine.slice(0, 60)}…` : firstLine;
  return fallback;
}

export function directoryName(directory: string | null): string | null {
  if (!directory) return null;
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

export function relativeTimeLabel(timestampMs: number, now: number = Date.now()): string {
  const elapsed = now - timestampMs;
  if (elapsed < MINUTE_MS) return "刚刚";
  const minutes = Math.floor(elapsed / MINUTE_MS);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(elapsed / HOUR_MS);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(elapsed / DAY_MS);
  if (days < 7) return `${days} 天前`;
  const date = new Date(timestampMs);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}
