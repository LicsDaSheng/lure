//! JSON 值导航辅助（对应 serde_json::Value 的 as_object/as_array/as_str/as_i64/as_u64）。

import type { Json } from "./types.js";

export function asObject(v: Json): Record<string, Json> | undefined {
  return v !== null && typeof v === "object" && !Array.isArray(v)
    ? (v as Record<string, Json>)
    : undefined;
}

export function asArray(v: Json): Json[] | undefined {
  return Array.isArray(v) ? v : undefined;
}

export function asString(v: Json): string | undefined {
  return typeof v === "string" ? v : undefined;
}

export function asNumber(v: Json): number | undefined {
  return typeof v === "number" ? v : undefined;
}
