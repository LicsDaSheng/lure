//! `stripThink`：移除思考块与模板级泄漏（对齐 crates/lure-core/src/memory/strip.rs）。

const WELL_FORMED = /<(think|thinking|thought)>.*?<\/(think|thinking|thought)>/gs;
const UNCLOSED_PREFIX = /^\s*<(think|thinking|thought)>.*$/s;
const CHANNEL_MARKER = /^\s*<\|?channel\|>\s*/;
const MALFORMED_OPEN = /<(think|thinking|thought)([^A-Za-z0-9_\-:>/]|$)/;

/// 移除思考块与模板级泄漏，随后去除尾部空白。
export function stripThink(text: string): string {
  let out = text.replace(WELL_FORMED, "");
  out = out.replace(UNCLOSED_PREFIX, "");
  out = out.replace(CHANNEL_MARKER, "");
  out = out.replace(MALFORMED_OPEN, "$2");
  return out.trimEnd();
}
