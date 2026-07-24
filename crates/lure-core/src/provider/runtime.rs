//! Stateful model runtime resolver：把 config 的 model preset 解析成不可变的
//! [`LlmRuntime`]/[`ProviderSnapshot`]，并按 preset 名缓存（preset tracking），
//! 提供 `admit`/`refresh`/`invalidate` 生命周期。
//!
//! 对齐上游 `tests/agent/test_model_runtime_resolver.py` 记录的语义（见
//! upstream-test-ledger）：
//! - `admit`：解析并选中某个 preset 为当前 runtime；命中缓存则复用同一快照。
//! - `refresh`：强制重建当前/指定 preset 的 runtime，`generation` 递增。
//! - `invalidate`：丢弃某 preset 的缓存；若正是当前 active，则清空 active。
//!
//! 解析产物是**不可变**的：每次 (re)build 捕获当时的 provider 身份与生成参数，
//! 之后 config 变化不会影响已发出的 [`LlmRuntime`] 副本，直到显式 `refresh`。
//! 真实网络调用不在此层，由 provider 传输负责。

use std::collections::HashMap;

use crate::config::{Config, PresetError, DEFAULT_PRESET_NAME};

use super::registry::match_provider;
use super::types::GenerationSettings;

/// 解析时刻捕获的 provider 身份快照（不可变）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSnapshot {
    /// registry provider 名（如 `deepseek`）。
    pub provider_name: String,
    /// provider 默认 API base URL。
    pub api_base: String,
    /// 解析后的模型标识。
    pub model: String,
}

/// 一次解析产出的不可变 LLM 运行时。
#[derive(Debug, Clone, PartialEq)]
pub struct LlmRuntime {
    /// 解析后的 canonical preset 名（`default` 或命名 preset）。
    pub preset_name: String,
    /// provider 身份快照。
    pub provider: ProviderSnapshot,
    /// 生成参数。
    pub settings: GenerationSettings,
    /// 该 preset 的构建代次；每次 (re)build 递增，用于追踪刷新。
    pub generation: u64,
}

/// resolver 结构化错误。
#[derive(Debug)]
pub enum RuntimeError {
    /// preset 解析失败（如命名 preset 不存在）。
    Preset(PresetError),
    /// 无法为该 model/provider 匹配到任何 registry provider。
    ProviderNotFound {
        /// 解析后的模型标识。
        model: String,
        /// forced provider 名或 `auto`。
        provider: String,
    },
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::Preset(err) => write!(f, "{err}"),
            RuntimeError::ProviderNotFound { model, provider } => {
                write!(
                    f,
                    "无法为 model '{model}'(provider='{provider}') 匹配 provider"
                )
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<PresetError> for RuntimeError {
    fn from(err: PresetError) -> Self {
        RuntimeError::Preset(err)
    }
}

/// stateful model runtime resolver。
#[derive(Debug)]
pub struct ModelRuntimeResolver {
    config: Config,
    /// 按 canonical preset 名缓存的 runtime（preset tracking）。
    cache: HashMap<String, LlmRuntime>,
    /// 当前 admitted 的 canonical preset 名。
    active: Option<String>,
    /// 全局单调代次计数，用于给每次 build 分配 `generation`。
    next_generation: u64,
}

impl ModelRuntimeResolver {
    /// 由 config 构造 resolver（初始无缓存、无 active）。
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cache: HashMap::new(),
            active: None,
            next_generation: 1,
        }
    }

    /// 当前 admitted 的 runtime（未 admit 过则为 `None`）。
    pub fn active(&self) -> Option<&LlmRuntime> {
        self.active.as_ref().and_then(|name| self.cache.get(name))
    }

    /// 解析并选中一个 preset 为当前 runtime。
    ///
    /// 命中缓存时复用同一不可变快照（`generation` 不变）；否则构建并缓存。
    /// `name` 为 `None` 时回落到 `agents.defaults.model_preset`。
    pub fn admit(&mut self, name: Option<&str>) -> Result<LlmRuntime, RuntimeError> {
        let canonical = self.canonical_name(name)?;
        if !self.cache.contains_key(&canonical) {
            let runtime = self.build(&canonical)?;
            self.cache.insert(canonical.clone(), runtime);
        }
        self.active = Some(canonical.clone());
        Ok(self.cache.get(&canonical).expect("just inserted").clone())
    }

    /// 强制重建某 preset 的 runtime 并选中它，`generation` 递增。
    ///
    /// `name` 为 `None` 时回落到 `agents.defaults.model_preset`。
    pub fn refresh(&mut self, name: Option<&str>) -> Result<LlmRuntime, RuntimeError> {
        let canonical = self.canonical_name(name)?;
        let runtime = self.build(&canonical)?;
        self.cache.insert(canonical.clone(), runtime.clone());
        self.active = Some(canonical);
        Ok(runtime)
    }

    /// 丢弃某 preset 的缓存；若正是当前 active，则清空 active。
    pub fn invalidate(&mut self, preset_name: &str) {
        self.cache.remove(preset_name);
        if self.active.as_deref() == Some(preset_name) {
            self.active = None;
        }
    }

    /// 把请求的 preset 名归一为 canonical 名（沿用 config 的回落/保留名规则）。
    fn canonical_name(&self, name: Option<&str>) -> Result<String, PresetError> {
        let requested = match name {
            Some(explicit) => Some(explicit.to_string()),
            None => self.config.agents.defaults.model_preset.clone(),
        };
        match requested.as_deref() {
            None | Some("") | Some(DEFAULT_PRESET_NAME) => Ok(DEFAULT_PRESET_NAME.to_string()),
            Some(other) if self.config.model_presets.contains_key(other) => Ok(other.to_string()),
            Some(other) => Err(PresetError::NotFound(other.to_string())),
        }
    }

    /// 由 canonical preset 名构建一个新的不可变 runtime，分配下一个 generation。
    fn build(&mut self, canonical: &str) -> Result<LlmRuntime, RuntimeError> {
        let requested = if canonical == DEFAULT_PRESET_NAME {
            None
        } else {
            Some(canonical)
        };
        let preset = self.config.resolve_preset(requested)?;

        let spec = match_provider(&preset.model, &preset.provider).ok_or_else(|| {
            RuntimeError::ProviderNotFound {
                model: preset.model.clone(),
                provider: preset.provider.clone(),
            }
        })?;

        let generation = self.next_generation;
        self.next_generation += 1;

        Ok(LlmRuntime {
            preset_name: canonical.to_string(),
            provider: ProviderSnapshot {
                provider_name: spec.name.to_string(),
                api_base: spec.default_api_base.to_string(),
                model: preset.model.clone(),
            },
            settings: GenerationSettings {
                temperature: preset.temperature,
                max_tokens: preset.max_tokens,
                reasoning_effort: preset.reasoning_effort.clone(),
            },
            generation,
        })
    }
}
