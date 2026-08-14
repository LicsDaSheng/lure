//! 最小 JSON Schema 参数校验（对齐 crates/lure-core/src/tool/schema.rs）。

import type { Json } from "../provider/types.js";
import { asObject } from "../provider/json.js";

function resolveType(schema: Record<string, Json>): [string, boolean] | undefined {
  const t = schema["type"];
  if (typeof t === "string") return [t, false];
  if (Array.isArray(t)) {
    const nullable = t.some((x) => x === "null");
    const name = t.find((x): x is string => typeof x === "string" && x !== "null");
    return name !== undefined ? [name, nullable] : undefined;
  }
  return undefined;
}

/// 校验 `value` 是否符合 schema 片段；返回错误消息（空表示通过）。
export function validateValue(value: Json, schema: Json, path: string): string[] {
  const label = path === "" ? "parameter" : path;
  const errors: string[] = [];
  const s = asObject(schema);
  if (s === undefined) return errors;
  const resolved = resolveType(s);
  if (resolved === undefined) return errors;
  const [typeName, nullable] = resolved;
  if (nullable && value === null) return errors;

  const typeOk =
    typeName === "string"
      ? typeof value === "string"
      : typeName === "integer"
        ? typeof value === "number" && Number.isInteger(value)
        : typeName === "number"
          ? typeof value === "number"
          : typeName === "boolean"
            ? typeof value === "boolean"
            : typeName === "array"
              ? Array.isArray(value)
              : typeName === "object"
                ? value !== null && typeof value === "object" && !Array.isArray(value)
                : true;
  if (!typeOk) {
    errors.push(`${label} should be ${typeName}`);
    return errors;
  }

  if (Array.isArray(s["enum"])) {
    if (!s["enum"].some((c) => jsonEqual(c, value))) {
      errors.push(`${label} must be one of the allowed values`);
    }
  }

  if (typeName === "integer" || typeName === "number") {
    if (typeof value === "number") {
      const min = typeof s["minimum"] === "number" ? s["minimum"] : undefined;
      if (min !== undefined && value < min) errors.push(`${label} must be >= ${min}`);
      const max = typeof s["maximum"] === "number" ? s["maximum"] : undefined;
      if (max !== undefined && value > max) errors.push(`${label} must be <= ${max}`);
    }
  } else if (typeName === "string") {
    const len = [...(typeof value === "string" ? value : "")].length;
    const min = typeof s["minLength"] === "number" ? s["minLength"] : undefined;
    if (min !== undefined && len < min) errors.push(`${label} must be at least ${min} chars`);
    const max = typeof s["maxLength"] === "number" ? s["maxLength"] : undefined;
    if (max !== undefined && len > max) errors.push(`${label} must be at most ${max} chars`);
  } else if (typeName === "object") {
    const obj = asObject(value) ?? {};
    if (Array.isArray(s["required"])) {
      for (const key of s["required"]) {
        if (typeof key === "string" && !(key in obj)) {
          errors.push(`missing required ${subpath(path, key)}`);
        }
      }
    }
    const props = asObject(s["properties"]);
    if (props !== undefined) {
      for (const [key, sub] of Object.entries(obj)) {
        const propSchema = props[key];
        if (propSchema !== undefined) {
          errors.push(...validateValue(sub, propSchema, subpath(path, key)));
        }
      }
    }
  } else if (typeName === "array") {
    const items = Array.isArray(value) ? value : [];
    const min = typeof s["minItems"] === "number" ? s["minItems"] : undefined;
    if (min !== undefined && items.length < min) errors.push(`${label} must have at least ${min} items`);
    const max = typeof s["maxItems"] === "number" ? s["maxItems"] : undefined;
    if (max !== undefined && items.length > max) errors.push(`${label} must have at most ${max} items`);
    const itemSchema = s["items"];
    if (itemSchema !== undefined) {
      for (let i = 0; i < items.length; i++) {
        const itemPath = path === "" ? `[${i}]` : `${path}[${i}]`;
        errors.push(...validateValue(items[i], itemSchema, itemPath));
      }
    }
  }

  return errors;
}

function subpath(path: string, key: string): string {
  return path === "" ? key : `${path}.${key}`;
}

function jsonEqual(a: Json, b: Json): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
