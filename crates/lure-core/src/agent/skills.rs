//! Agent skills 装载器。
//!
//! 对齐上游 `nanobot/agent/skills.py`：skills 为 `SKILL.md`（含 YAML frontmatter）的目录，
//! 教 agent 如何使用特定工具或完成任务。两源枚举（workspace 优先遮蔽 builtin 同名）、按
//! `requires.{bins,env}` 过滤不可用、disabled 排除、渐进式摘要（`build_skills_summary`）。
//!
//! frontmatter 用 YAML 解析（支持 flow mapping / 折叠 `>` / 字面 `|` / 原生类型），反序列化为
//! `serde_json::Value` 供下游统一消费。需求探针（which/env）以注入方式替代上游 `shutil.which` /
//! `os.environ`，便于确定性测试。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use serde_json::Value;

/// 一条 skill 的枚举信息（对齐上游 entry dict 的 `name`/`path`/`source`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub name: String,
    /// `SKILL.md` 的绝对路径字符串。
    pub path: String,
    /// 来源：`"workspace"` 或 `"builtin"`。
    pub source: String,
}

/// skill 显式需求与当前缺失项（对齐上游 `get_skill_requirements`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRequirements {
    pub bins: Vec<String>,
    pub env: Vec<String>,
    pub missing_bins: Vec<String>,
    pub missing_env: Vec<String>,
}

/// 二进制是否存在的探针（对齐 `shutil.which`）。
type WhichProbe = Arc<dyn Fn(&str) -> bool + Send + Sync>;
/// 环境变量是否已设置且非空的探针（对齐 `os.environ.get` 真值判断）。
type EnvProbe = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Agent skills 装载器。
pub struct SkillsLoader {
    workspace_skills: PathBuf,
    builtin_skills: Option<PathBuf>,
    disabled_skills: BTreeSet<String>,
    which: WhichProbe,
    env: EnvProbe,
}

impl SkillsLoader {
    /// 以 workspace 根 + 可选 builtin skills 目录 + disabled 集合构造。
    ///
    /// 默认需求探针：`which` 走 PATH 查找可执行文件，`env` 走进程环境变量真值判断。
    pub fn new(
        workspace: impl AsRef<Path>,
        builtin_skills_dir: Option<PathBuf>,
        disabled_skills: BTreeSet<String>,
    ) -> Self {
        Self {
            workspace_skills: workspace.as_ref().join("skills"),
            builtin_skills: builtin_skills_dir,
            disabled_skills,
            which: Arc::new(default_which),
            env: Arc::new(default_env),
        }
    }

    /// 注入自定义需求探针（测试用；替代上游 monkeypatch）。
    pub fn with_probes(mut self, which: WhichProbe, env: EnvProbe) -> Self {
        self.which = which;
        self.env = env;
        self
    }

    /// 枚举可用 skills。`filter_unavailable=true` 时剔除需求未满足的项。
    pub fn list_skills(&self, filter_unavailable: bool) -> Vec<SkillEntry> {
        let mut skills = entries_from_dir(&self.workspace_skills, "workspace", None);
        let workspace_names: BTreeSet<String> = skills.iter().map(|e| e.name.clone()).collect();
        if let Some(builtin) = self.builtin_skills.as_ref() {
            if builtin.exists() {
                skills.extend(entries_from_dir(builtin, "builtin", Some(&workspace_names)));
            }
        }

        if !self.disabled_skills.is_empty() {
            skills.retain(|e| !self.disabled_skills.contains(&e.name));
        }

        if filter_unavailable {
            skills.retain(|e| self.check_requirements(&self.skill_meta(&e.name)));
        }
        skills
    }

    /// 按名加载 skill 内容（workspace 优先，其次 builtin）；不存在返回 `None`。
    pub fn load_skill(&self, name: &str) -> Option<String> {
        let mut roots = vec![self.workspace_skills.clone()];
        if let Some(builtin) = self.builtin_skills.as_ref() {
            roots.push(builtin.clone());
        }
        for root in roots {
            let path = root.join(name).join("SKILL.md");
            if path.exists() {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    return Some(text);
                }
            }
        }
        None
    }

    /// 加载指定 skills 用于注入 agent 上下文（剥离 frontmatter，段间以 `---` 分隔）。
    pub fn load_skills_for_context(&self, skill_names: &[String]) -> String {
        let parts: Vec<String> = skill_names
            .iter()
            .filter_map(|name| {
                self.load_skill(name)
                    .map(|md| format!("### Skill: {name}\n\n{}", strip_frontmatter(&md)))
            })
            .collect();
        parts.join("\n\n---\n\n")
    }

    /// 从当前用户文本解析 `$skill-name`，只返回已启用且依赖满足的 skill，保持首次出现顺序。
    pub fn get_explicitly_invoked_skills(&self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        let available: BTreeSet<String> = self
            .list_skills(true)
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        let mut invoked = Vec::new();
        for captures in skill_reference_regex().captures_iter(text) {
            let Some(name) = captures.get(1).map(|value| value.as_str()) else {
                continue;
            };
            if available.contains(name) && !invoked.iter().any(|item| item == name) {
                invoked.push(name.to_string());
            }
        }
        invoked
    }

    /// 构建全部 skills 的摘要（名称、描述、可用性、相对路径），用于渐进式加载。
    pub fn build_skills_summary(&self, exclude: Option<&BTreeSet<String>>) -> String {
        let all = self.list_skills(false);
        if all.is_empty() {
            return String::new();
        }
        let empty = BTreeSet::new();
        let exclude = exclude.unwrap_or(&empty);

        let groups: [(&str, &str, Option<&PathBuf>); 2] = [
            (
                "Workspace skills",
                "workspace",
                Some(&self.workspace_skills),
            ),
            ("Built-in skills", "builtin", self.builtin_skills.as_ref()),
        ];

        let mut sections: Vec<String> = Vec::new();
        for (label, source, root) in groups {
            let Some(root) = root else { continue };
            let entries: Vec<&SkillEntry> = all
                .iter()
                .filter(|e| e.source == source && !exclude.contains(&e.name))
                .collect();
            if entries.is_empty() {
                continue;
            }
            let mut lines = vec![format!("### {label} (`{}`)", root.display())];
            for e in entries {
                let meta = self.skill_meta(&e.name);
                let available = self.check_requirements(&meta);
                let desc = self.skill_description(&e.name);
                let suffix = if available {
                    String::new()
                } else {
                    let missing = self.missing_requirements(&meta);
                    if missing.is_empty() {
                        " (unavailable)".to_string()
                    } else {
                        format!(" (unavailable: {missing})")
                    }
                };
                let relative = Path::new(&e.path)
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| e.path.clone());
                lines.push(format!("- **{}** — {desc}{suffix}  `{relative}`", e.name));
            }
            sections.push(lines.join("\n"));
        }
        sections.join("\n\n")
    }

    /// 返回被标记 `always=true` 且满足需求的 skill 名列表。
    pub fn get_always_skills(&self) -> Vec<String> {
        self.list_skills(true)
            .into_iter()
            .filter(|e| {
                let Some(meta) = self.get_skill_metadata(&e.name) else {
                    return false;
                };
                let nanobot_always = parse_nanobot_metadata(meta.get("metadata"))
                    .get("always")
                    .map(is_truthy)
                    .unwrap_or(false);
                let top_always = meta.get("always").map(is_truthy).unwrap_or(false);
                nanobot_always || top_always
            })
            .map(|e| e.name)
            .collect()
    }

    /// 返回 skill 是否可运行，以及不可运行时的原因。
    pub fn get_skill_availability(&self, name: &str) -> (bool, String) {
        let meta = self.skill_meta(name);
        let available = self.check_requirements(&meta);
        let reason = if available {
            String::new()
        } else {
            self.missing_requirements(&meta)
        };
        (available, reason)
    }

    /// 返回 skill 的显式命令/环境需求及当前缺失项。
    pub fn get_skill_requirements(&self, name: &str) -> SkillRequirements {
        let meta = self.skill_meta(name);
        let bins = string_list(meta.get("requires").and_then(|r| r.get("bins")));
        let env = string_list(meta.get("requires").and_then(|r| r.get("env")));
        let missing_bins = bins.iter().filter(|b| !(self.which)(b)).cloned().collect();
        let missing_env = env.iter().filter(|e| !(self.env)(e)).cloned().collect();
        SkillRequirements {
            bins,
            env,
            missing_bins,
            missing_env,
        }
    }

    /// 从 skill frontmatter 读取元数据（YAML → JSON Value）；无 frontmatter 返回 `None`。
    pub fn get_skill_metadata(&self, name: &str) -> Option<Value> {
        let content = self.load_skill(name)?;
        parse_frontmatter(&content)
    }

    // —— 内部 —— //

    /// 提取 skill 的 nanobot/openclaw 元数据 payload（供需求判断）。
    fn skill_meta(&self, name: &str) -> Value {
        let meta = self.get_skill_metadata(name).unwrap_or(Value::Null);
        parse_nanobot_metadata(meta.get("metadata"))
    }

    /// 返回 skill 的描述（frontmatter `description`，缺省回落 skill 名）。
    pub fn skill_description(&self, name: &str) -> String {
        self.get_skill_metadata(name)
            .and_then(|m| {
                m.get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| name.to_string())
    }

    fn check_requirements(&self, skill_meta: &Value) -> bool {
        let bins = string_list(skill_meta.get("requires").and_then(|r| r.get("bins")));
        let env = string_list(skill_meta.get("requires").and_then(|r| r.get("env")));
        bins.iter().all(|b| (self.which)(b)) && env.iter().all(|e| (self.env)(e))
    }

    fn missing_requirements(&self, skill_meta: &Value) -> String {
        let bins = string_list(skill_meta.get("requires").and_then(|r| r.get("bins")));
        let env = string_list(skill_meta.get("requires").and_then(|r| r.get("env")));
        let mut parts: Vec<String> = bins
            .into_iter()
            .filter(|b| !(self.which)(b))
            .map(|b| format!("CLI: {b}"))
            .collect();
        parts.extend(
            env.into_iter()
                .filter(|e| !(self.env)(e))
                .map(|e| format!("ENV: {e}")),
        );
        parts.join(", ")
    }
}

/// `$skill-name` 引用；避免把 `$HOME` 等普通 shell 变量激活为不存在的 skill。
fn skill_reference_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\$([A-Za-z0-9][A-Za-z0-9_-]*)").unwrap())
}

/// 枚举目录下每个含 `SKILL.md` 的子目录。`skip_names` 用于 builtin 跳过 workspace 已有名。
fn entries_from_dir(base: &Path, source: &str, skip: Option<&BTreeSet<String>>) -> Vec<SkillEntry> {
    if !base.exists() {
        return Vec::new();
    }
    let Ok(read) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for dir_entry in read.flatten() {
        let skill_dir = dir_entry.path();
        if !skill_dir.is_dir() {
            continue;
        }
        let skill_file = skill_dir.join("SKILL.md");
        if !skill_file.exists() {
            continue;
        }
        let name = skill_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if skip.is_some_and(|s| s.contains(&name)) {
            continue;
        }
        entries.push(SkillEntry {
            name,
            path: skill_file.to_string_lossy().into_owned(),
            source: source.to_string(),
        });
    }
    entries
}

/// frontmatter 正则：起始 `---`、YAML 体（组 1）、独占一行的结束 `---`，支持 CRLF。
fn frontmatter_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)^---[ \t]*\r?\n(.*?)\r?\n---[ \t]*\r?\n?").unwrap())
}

/// 解析 frontmatter 为 JSON Value；无 `---` 开头或非映射返回 `None`。
fn parse_frontmatter(content: &str) -> Option<Value> {
    if !content.starts_with("---") {
        return None;
    }
    let caps = frontmatter_regex().captures(content)?;
    let body = caps.get(1)?.as_str();
    let parsed: Value = serde_yaml::from_str(body).ok()?;
    if parsed.is_object() {
        Some(parsed)
    } else {
        None
    }
}

/// 剥离 markdown 的 YAML frontmatter，返回去首尾空白的正文。
fn strip_frontmatter(content: &str) -> String {
    if !content.starts_with("---") {
        return content.to_string();
    }
    match frontmatter_regex().find(content) {
        Some(m) => content[m.end()..].trim().to_string(),
        None => content.to_string(),
    }
}

/// 从 frontmatter 的 `metadata` 字段提取 nanobot/openclaw payload。
///
/// `raw` 可能是对象（YAML flow mapping 已解析）或 JSON 字符串；取 `nanobot` 优先 `openclaw`。
fn parse_nanobot_metadata(raw: Option<&Value>) -> Value {
    let data = match raw {
        Some(Value::Object(_)) => raw.cloned().unwrap(),
        Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
            Ok(v) if v.is_object() => v,
            _ => return Value::Object(Default::default()),
        },
        _ => return Value::Object(Default::default()),
    };
    data.get("nanobot")
        .or_else(|| data.get("openclaw"))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()))
}

/// 把 JSON 值当作字符串列表读取（非数组或非字符串项忽略）。
fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// YAML/JSON 真值判断（bool true 或非空字符串/非零）。
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// 默认 `which`：在 PATH 各目录查找同名可执行文件。
fn default_which(cmd: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let candidate = dir.join(cmd);
        candidate.is_file()
    })
}

/// 默认 `env`：环境变量存在且非空。
fn default_env(name: &str) -> bool {
    std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false)
}
