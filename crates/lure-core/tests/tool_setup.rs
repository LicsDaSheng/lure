//! config 驱动的工具注册：`registry_from_config` 默认注册 workspace 绑定的文件工具，
//! exec 工具按 `tools.exec` 开关 + allow/deny 门禁注册。
//!
//! 让 CLI/WebUI 等前端共用同一注册策略；此处直接单测，不触网。

use std::path::Path;

use lure_core::config::Config;
use lure_core::tool::registry_from_config;

#[test]
fn registers_workspace_file_tools_by_default() {
    let registry = registry_from_config(&Config::default(), Path::new("/tmp/ws")).unwrap();
    assert!(registry.contains("read_file"));
    assert!(registry.contains("write_file"));
    // exec 默认关闭。
    assert!(!registry.contains("exec"));
}

#[test]
fn registers_exec_tool_when_enabled() {
    let mut config = Config::default();
    config.tools.exec.enabled = true;
    config.tools.exec.allow = vec!["^ls".to_string()];

    let registry = registry_from_config(&config, Path::new("/tmp/ws")).unwrap();
    assert!(registry.contains("exec"));
    assert!(registry.contains("read_file"));
}

#[test]
fn invalid_exec_pattern_is_error() {
    let mut config = Config::default();
    config.tools.exec.enabled = true;
    config.tools.exec.allow = vec!["[".to_string()]; // 非法正则

    assert!(registry_from_config(&config, Path::new("/tmp/ws")).is_err());
}

#[test]
fn tools_config_deserializes_camel_and_defaults() {
    let json = r#"{"tools":{"exec":{"enabled":true,"allow":["^ls"],"deny":["rm"]}}}"#;
    let config: Config = serde_json::from_str(json).unwrap();
    assert!(config.tools.exec.enabled);
    assert_eq!(config.tools.exec.allow, vec!["^ls".to_string()]);
    assert_eq!(config.tools.exec.deny, vec!["rm".to_string()]);
    // 缺省 config 下 exec 关闭。
    assert!(!Config::default().tools.exec.enabled);
}
