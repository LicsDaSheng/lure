//! `lure-cli` 的共享库：agent loop 构建逻辑（CLI 与 desktop 复用）。
//!
//! provider 选择统一经 `ModelRuntimeResolver`：`--preset`/`--model` 互斥；
//! `--model echo` 显式启用离线 EchoProvider 测试脚手架。

use std::path::{Path, PathBuf};

use lure_core::agent::{AgentLoop, ContextBuilder};
use lure_core::config::{default_config_path, load_config, Config};
use lure_core::memory::MemoryStore;
use lure_core::provider::{
    EchoProvider, LlmProvider, LlmRuntime, ModelRuntimeResolver, OpenAiCompatProvider,
    ReqwestTransport,
};
use lure_core::session::SessionManager;
use lure_core::tool::registry_from_config;

/// 构建挂载工具运行时与长期记忆的 agent loop。
///
/// `config_path` 缺省用 `default_config_path`；`preset` 与 `model` 互斥。
pub fn build_agent_loop(
    config_path: Option<&str>,
    preset: Option<&str>,
    model: Option<&str>,
    workspace: &Path,
    sessions: SessionManager,
) -> Result<AgentLoop, String> {
    let context = ContextBuilder::new(None);
    // 长期记忆是核心能力，两个分支都挂载：注入记忆块 + 记录 history.jsonl。
    let memory = MemoryStore::new(workspace).map_err(|e| format!("初始化 memory 失败: {e}"))?;

    let config = load_cli_config(config_path)?;
    if preset.is_some() && model.is_some() {
        return Err("--preset 与 --model 互斥，只能二选一".to_string());
    }

    // 工具运行时先于 provider 构建：config 里的 exec 策略正则等错误 fail-fast，
    // 不必等到读取 API key / 出网。
    let tools = registry_from_config(&config, workspace).map_err(|e| e.to_string())?;
    if model == Some("echo") {
        return Ok(
            AgentLoop::new(Box::new(EchoProvider::new()), sessions, context)
                .with_tools(tools)
                .with_memory(memory),
        );
    }

    let runtime = resolve_runtime(config.clone(), preset, model)?;
    let provider = build_provider_from_runtime(&config, &runtime)?;
    Ok(AgentLoop::new(provider, sessions, context)
        .with_runtime(&runtime)
        .with_tools(tools)
        .with_memory(memory))
}

/// 解析出与 chat 同源的独立 provider 实例（供 dream 等复用）。
///
/// 与 [`build_agent_loop`] 的 provider 选择完全一致：`--preset` 与 `--model` 互斥；
/// `--model echo` 显式返回离线 [`EchoProvider`]；否则由 config/preset/model 解析出
/// 真实 provider。用于让 desktop 的 dream consolidation 走真实 LLM 而非离线 Echo。
pub fn build_provider(
    config_path: Option<&str>,
    preset: Option<&str>,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider + Send>, String> {
    if preset.is_some() && model.is_some() {
        return Err("--preset 与 --model 互斥，只能二选一".to_string());
    }
    if model == Some("echo") {
        return Ok(Box::new(EchoProvider::new()));
    }
    let config = load_cli_config(config_path)?;
    let runtime = resolve_runtime(config.clone(), preset, model)?;
    build_provider_from_runtime(&config, &runtime)
}

/// 加载 config 文件（缺省用 `default_config_path`）；文件不存在时回落到默认配置。
pub fn load_cli_config(config_path: Option<&str>) -> Result<Config, String> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    load_config(&path).map_err(|e| e.to_string())
}

/// 从 config + flag 解析出不可变 runtime。
///
/// `--preset` 与 `--model` 互斥：`--preset` 选中命名 preset；`--model` 覆盖默认 preset
/// 的 model（并强制 `provider=auto` 走 registry 匹配）。
pub fn resolve_runtime(
    mut config: Config,
    preset: Option<&str>,
    model: Option<&str>,
) -> Result<LlmRuntime, String> {
    let selected = match (preset, model) {
        (Some(_), Some(_)) => return Err("--preset 与 --model 互斥，只能二选一".to_string()),
        (Some(name), None) => Some(name.to_string()),
        (None, Some(model)) => {
            config.agents.defaults.model = model.to_string();
            config.agents.defaults.provider = "auto".to_string();
            None
        }
        (None, None) => None,
    };

    ModelRuntimeResolver::new(config)
        .admit(selected.as_deref())
        .map_err(|e| e.to_string())
}

/// 由 runtime 的 provider 快照构造真实 provider。
///
/// api_key 解析：优先 `config.providers.<name>.apiKey`，否则回落环境变量
/// `<PROVIDER>_API_KEY`；api_base/model 取 runtime 快照（已含 config 覆盖）。
pub fn build_provider_from_runtime(
    config: &Config,
    runtime: &LlmRuntime,
) -> Result<Box<dyn LlmProvider + Send>, String> {
    let provider_name = &runtime.provider.provider_name;
    let env_key = format!("{}_API_KEY", provider_name.to_uppercase());
    let api_key = config
        .provider_api_key(provider_name)
        .or_else(|| std::env::var(&env_key).ok())
        .ok_or_else(|| {
            format!("缺少 API key：config.providers.{provider_name}.apiKey 或环境变量 {env_key}")
        })?;

    Ok(Box::new(OpenAiCompatProvider::new(
        &runtime.provider.api_base,
        Some(api_key),
        &runtime.provider.model,
        ReqwestTransport::new(),
    )))
}
