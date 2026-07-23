//! 工具 trait 与注册表。
//!
//! 对齐上游 `Tool`/`ToolRegistry`：工具暴露 name/description/parameters(JSON Schema)/
//! execute；注册表按名派发、产出 OpenAI function-calling 定义，并在调用前做参数校验。
//!
//! Phase 5 采用**同步** execute（上游为 async）；MCP 排序、near-miss 建议只做最小实现。

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{json, Value};

use crate::tool::result::ToolResult;
use crate::tool::schema::validate_value;

/// 工具契约。
pub trait Tool {
    /// 工具名（function call 中使用）。
    fn name(&self) -> &str;
    /// 工具描述。
    fn description(&self) -> &str;
    /// 参数 JSON Schema。
    fn parameters(&self) -> Value;
    /// 是否无副作用、可并行。
    fn read_only(&self) -> bool {
        false
    }
    /// 执行工具。
    fn execute(&self, args: &Value) -> ToolResult;
}

/// 工具调用错误。
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// 工具名不存在（可能给出近似建议）。
    UnknownTool {
        /// 请求的工具名。
        name: String,
        /// 近似的已注册工具名。
        suggestion: Option<String>,
    },
    /// 参数未通过 schema 校验。
    InvalidArgs {
        /// 工具名。
        name: String,
        /// 校验错误列表。
        errors: Vec<String>,
    },
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::UnknownTool { name, suggestion } => match suggestion {
                Some(s) => write!(f, "未知工具 '{name}'，是否想调用 '{s}'?"),
                None => write!(f, "未知工具 '{name}'"),
            },
            ToolError::InvalidArgs { name, errors } => {
                write!(f, "工具 '{name}' 参数无效: {}", errors.join("; "))
            }
        }
    }
}

impl std::error::Error for ToolError {}

/// 工具注册表。
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn Tool>>,
    order: Vec<String>,
}

impl ToolRegistry {
    /// 新建空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册工具（同名覆盖，保持首次注册顺序）。
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        if !self.tools.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.tools.insert(name, tool);
    }

    /// 已注册工具名（注册顺序）。
    pub fn names(&self) -> Vec<&str> {
        self.order.iter().map(String::as_str).collect()
    }

    /// 是否包含某工具。
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// 产出 OpenAI function-calling 定义（注册顺序）。
    pub fn get_definitions(&self) -> Vec<Value> {
        self.order
            .iter()
            .filter_map(|name| self.tools.get(name))
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters(),
                    }
                })
            })
            .collect()
    }

    /// 按名校验参数并派发执行；未知工具或参数无效返回结构化错误。
    pub fn execute(&self, name: &str, args: &Value) -> Result<ToolResult, ToolError> {
        let Some(tool) = self.tools.get(name) else {
            return Err(ToolError::UnknownTool {
                name: name.to_string(),
                suggestion: self.suggest(name),
            });
        };

        let errors = validate_value(args, &tool.parameters(), "");
        if !errors.is_empty() {
            return Err(ToolError::InvalidArgs {
                name: name.to_string(),
                errors,
            });
        }

        Ok(tool.execute(args))
    }

    /// 近似匹配：忽略大小写与下划线/连字符差异。
    fn suggest(&self, name: &str) -> Option<String> {
        let target = normalize(name);
        self.order
            .iter()
            .find(|registered| normalize(registered) == target)
            .cloned()
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}
