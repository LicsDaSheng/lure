//! 最小 JSON Schema 参数校验。
//!
//! 对齐上游 `Schema.validate_json_schema_value` 的核心子集：类型、required、enum、
//! 数值 min/max、字符串长度、对象/数组递归。complex 组合（oneOf/anyOf、pattern、
//! additionalProperties 深校验）留待需要时补齐。

use serde_json::Value;

/// 解析非 null 的类型名（`["string","null"]` -> `"string"`）。
fn resolve_type(schema: &Value) -> Option<(String, bool)> {
    match schema.get("type") {
        Some(Value::String(t)) => Some((t.clone(), false)),
        Some(Value::Array(types)) => {
            let nullable = types.iter().any(|t| t.as_str() == Some("null"));
            let name = types
                .iter()
                .find_map(|t| t.as_str().filter(|s| *s != "null"))
                .map(str::to_string);
            name.map(|n| (n, nullable))
        }
        _ => None,
    }
}

/// 校验 `value` 是否符合 schema 片段；返回错误消息（空表示通过）。
pub fn validate_value(value: &Value, schema: &Value, path: &str) -> Vec<String> {
    let label = if path.is_empty() { "parameter" } else { path };
    let mut errors = Vec::new();

    let Some((type_name, nullable)) = resolve_type(schema) else {
        return errors;
    };
    if nullable && value.is_null() {
        return errors;
    }

    let type_ok = match type_name.as_str() {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    };
    if !type_ok {
        errors.push(format!("{label} should be {type_name}"));
        return errors;
    }

    if let Some(Value::Array(choices)) = schema.get("enum") {
        if !choices.contains(value) {
            errors.push(format!("{label} must be one of the allowed values"));
        }
    }

    match type_name.as_str() {
        "integer" | "number" => {
            if let Some(n) = value.as_f64() {
                if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
                    if n < min {
                        errors.push(format!("{label} must be >= {min}"));
                    }
                }
                if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
                    if n > max {
                        errors.push(format!("{label} must be <= {max}"));
                    }
                }
            }
        }
        "string" => {
            let len = value.as_str().map(|s| s.chars().count()).unwrap_or(0);
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if (len as u64) < min {
                    errors.push(format!("{label} must be at least {min} chars"));
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if (len as u64) > max {
                    errors.push(format!("{label} must be at most {max} chars"));
                }
            }
        }
        "object" => {
            let obj = value.as_object().expect("已校验为 object");
            if let Some(Value::Array(required)) = schema.get("required") {
                for key in required.iter().filter_map(Value::as_str) {
                    if !obj.contains_key(key) {
                        errors.push(format!("missing required {}", subpath(path, key)));
                    }
                }
            }
            if let Some(Value::Object(props)) = schema.get("properties") {
                for (key, sub) in obj {
                    if let Some(prop_schema) = props.get(key) {
                        errors.extend(validate_value(sub, prop_schema, &subpath(path, key)));
                    }
                }
            }
        }
        "array" => {
            let items = value.as_array().expect("已校验为 array");
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                if (items.len() as u64) < min {
                    errors.push(format!("{label} must have at least {min} items"));
                }
            }
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                if (items.len() as u64) > max {
                    errors.push(format!("{label} must have at most {max} items"));
                }
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    let item_path = if path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{path}[{i}]")
                    };
                    errors.extend(validate_value(item, item_schema, &item_path));
                }
            }
        }
        _ => {}
    }

    errors
}

fn subpath(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}
