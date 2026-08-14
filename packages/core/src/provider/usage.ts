//! Usage 归一（对齐 crates/lure-core/src/provider/usage.rs）。

import type { Json } from "./types.js";
import { asNumber, asObject } from "./json.js";

/// cached_tokens 的优先级路径链：首个非零命中即采用。
const CACHED_TOKEN_PATHS: string[][] = [
  ["prompt_tokens_details", "cached_tokens"],
  ["cached_tokens"],
  ["prompt_cache_hit_tokens"],
];

/// 把原始 usage 归一为 `{prompt_tokens, completion_tokens, total_tokens[, cached_tokens]}`。
export function normalizeUsage(raw: Record<string, Json>): Record<string, Json> {
  if (Object.keys(raw).length === 0) return {};

  const result: Record<string, Json> = {};
  for (const key of ["prompt_tokens", "completion_tokens", "total_tokens"]) {
    result[key] = asNumber(raw[key]) ?? 0;
  }

  for (const path of CACHED_TOKEN_PATHS) {
    const cached = getNestedInt(raw, path);
    if (cached !== 0) {
      result["cached_tokens"] = cached;
      break;
    }
  }

  return result;
}

/// 沿 `path` 逐段下钻取整数；任一段缺失或非对象返回 0。
function getNestedInt(root: Record<string, Json>, path: string[]): number {
  const last = path[path.length - 1];
  const parents = path.slice(0, -1);
  let current: Record<string, Json> | undefined = root;
  for (const segment of parents) {
    current = asObject(current?.[segment]);
    if (current === undefined) return 0;
  }
  return asNumber(current[last!]) ?? 0;
}
