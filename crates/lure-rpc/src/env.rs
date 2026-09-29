//! Pi 子进程环境合成。
//!
//! GUI 方式启动的 Lure 不加载用户 shell 启动文件，导致 `~/.zshrc.local`
//! 中的 API key 与代理导出对 Pi 子进程不可见。本模块在 spawn 前合成完整
//! 环境：登录 shell 环境捕获（含代理导出）+ macOS 系统代理（scutil）兜底，
//! 再覆盖运行时 PATH。

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use tokio::sync::OnceCell;

/// 捕获输出中隔离 shell rc 噪音的哨兵行。
const ENV_SENTINEL: &str = "__LURE_ENV_7F3A9C_BEGIN__";

/// 登录 shell 环境捕获的超时上限。
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);

/// 环境变量条目。
pub type EnvVars = Vec<(OsString, OsString)>;

/// Pi 子进程的环境来源。
#[derive(Debug, Clone, Default)]
pub enum SpawnEnv {
    /// 继承 Lure 进程环境（仅覆盖 PATH），保持既有行为。
    #[default]
    Inherit,
    /// 使用合成环境完整替换子进程环境。
    Provided(EnvVars),
}

/// 环境合成器：登录 shell 环境捕获一次并缓存，系统代理每次解析时重读。
#[derive(Debug, Default)]
pub struct SpawnEnvResolver {
    shell_env: OnceCell<Option<EnvVars>>,
}

impl SpawnEnvResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 合成一份完整的 Pi 子进程环境。
    ///
    /// shell 环境捕获失败时回退为继承当前进程环境，不劣于既有行为。
    pub async fn resolve(&self) -> EnvVars {
        let base = self
            .shell_env
            .get_or_init(|| async { capture_login_shell_env(CAPTURE_TIMEOUT).await })
            .await
            .clone()
            .unwrap_or_else(|| std::env::vars_os().collect());
        let home = lookup(&base, "HOME");
        let path = lookup(&base, "PATH");
        let runtime_path = pi_runtime_path(home.as_deref(), path.as_deref());
        let proxy = system_proxy_env().await;
        build_spawn_env(base, &proxy, runtime_path)
    }
}

fn lookup(vars: &EnvVars, key: &str) -> Option<OsString> {
    vars.iter()
        .find(|(name, _)| name == OsStr::new(key))
        .map(|(_, value)| value.clone())
}

/// 捕获用户登录 shell 的完整环境。
///
/// 通过 `$SHELL -lic` 执行「哨兵行 + `env -0`」，以 NUL 分隔解析，
/// 隔离 rc 文件向 stdout 输出的噪音。失败（无 shell、超时、输出异常）
/// 返回 `None`，由调用方回退为继承环境。
pub async fn capture_login_shell_env(timeout: Duration) -> Option<EnvVars> {
    capture_login_shell_env_with(
        std::env::var_os("SHELL").map(std::path::PathBuf::from),
        &[],
        timeout,
    )
    .await
}

async fn capture_login_shell_env_with(
    shell: Option<std::path::PathBuf>,
    extra_env: &[(OsString, OsString)],
    timeout: Duration,
) -> Option<EnvVars> {
    let shell = shell?;
    let script = format!("printf '%s\\n' {ENV_SENTINEL}; command env -0");
    let mut command = tokio::process::Command::new(shell);
    command
        .args(["-l", "-i", "-c", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    for (name, value) in extra_env {
        command.env(name, value);
    }
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_captured_env(&output.stdout)
}

/// 解析 shell 捕获输出：哨兵行之后为 `env -0` 的 NUL 分隔条目。
fn parse_captured_env(output: &[u8]) -> Option<EnvVars> {
    let marker = format!("{ENV_SENTINEL}\n");
    let start = find_subsequence(output, marker.as_bytes())? + marker.len();
    let entries: Vec<(OsString, OsString)> = output[start..]
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let eq = entry.iter().position(|byte| *byte == b'=')?;
            Some((os_string(&entry[..eq]), os_string(&entry[eq + 1..])))
        })
        .collect();
    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(unix)]
fn os_string(bytes: &[u8]) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes.to_vec())
}

#[cfg(not(unix))]
fn os_string(bytes: &[u8]) -> OsString {
    OsString::from(String::from_utf8_lossy(bytes).into_owned())
}

/// macOS 系统代理配置。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemProxy {
    pub http: Option<(String, u16)>,
    pub https: Option<(String, u16)>,
    pub socks: Option<(String, u16)>,
    pub exceptions: Vec<String>,
}

impl SystemProxy {
    /// 映射为代理环境变量（大小写双写，兼容 Node 生态的大小写差异）。
    #[must_use]
    pub fn to_env_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some((host, port)) = &self.http {
            let url = format!("http://{host}:{port}");
            pairs.push(("HTTP_PROXY".to_string(), url.clone()));
            pairs.push(("http_proxy".to_string(), url));
        }
        if let Some((host, port)) = &self.https {
            let url = format!("http://{host}:{port}");
            pairs.push(("HTTPS_PROXY".to_string(), url.clone()));
            pairs.push(("https_proxy".to_string(), url));
        }
        if let Some((host, port)) = &self.socks {
            let url = format!("socks5://{host}:{port}");
            pairs.push(("ALL_PROXY".to_string(), url.clone()));
            pairs.push(("all_proxy".to_string(), url));
        }
        let exceptions: Vec<String> = self
            .exceptions
            .iter()
            .filter_map(|entry| {
                if entry == "<local>" {
                    return None;
                }
                Some(
                    entry
                        .strip_prefix("*.")
                        .map_or_else(|| entry.clone(), |rest| format!(".{rest}")),
                )
            })
            .collect();
        if !exceptions.is_empty() && !pairs.is_empty() {
            let joined = exceptions.join(",");
            pairs.push(("NO_PROXY".to_string(), joined.clone()));
            pairs.push(("no_proxy".to_string(), joined));
        }
        pairs
    }
}

/// 解析 `scutil --proxy` 的输出。
///
/// 需要认证的代理（`*ProxyAuthenticated : 1`）无法取得凭据，按未启用处理。
#[must_use]
pub fn parse_scutil_proxy(output: &str) -> SystemProxy {
    let mut proxy = SystemProxy::default();
    let mut exceptions_started = false;
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("ExceptionsList") {
            exceptions_started = true;
            continue;
        }
        if line.starts_with('}') {
            exceptions_started = false;
            continue;
        }
        if exceptions_started {
            if let Some((_, value)) = line.split_once(" : ") {
                proxy.exceptions.push(value.trim().to_string());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(" : ") else {
            continue;
        };
        let value = value.trim();
        match key {
            "HTTPEnable" if value == "1" => proxy.http = Some((String::new(), 0)),
            "HTTPProxy" => set_host(&mut proxy.http, value),
            "HTTPPort" => set_port(&mut proxy.http, value),
            "HTTPProxyAuthenticated" if value == "1" => proxy.http = None,
            "HTTPSEnable" if value == "1" => proxy.https = Some((String::new(), 0)),
            "HTTPSProxy" => set_host(&mut proxy.https, value),
            "HTTPSPort" => set_port(&mut proxy.https, value),
            "HTTPSProxyAuthenticated" if value == "1" => proxy.https = None,
            "SOCKSEnable" if value == "1" => proxy.socks = Some((String::new(), 0)),
            "SOCKSProxy" => set_host(&mut proxy.socks, value),
            "SOCKSPort" => set_port(&mut proxy.socks, value),
            _ => {}
        }
    }
    // scutil 输出中 Enable 可能先于 Proxy/Port 出现；清理缺主机或端口的半成品。
    prune(&mut proxy.http);
    prune(&mut proxy.https);
    prune(&mut proxy.socks);
    proxy
}

fn set_host(slot: &mut Option<(String, u16)>, host: &str) {
    if let Some(entry) = slot {
        entry.0 = host.to_string();
    }
}

fn set_port(slot: &mut Option<(String, u16)>, port: &str) {
    if let (Some(entry), Ok(port)) = (slot, port.parse::<u16>()) {
        entry.1 = port;
    }
}

fn prune(slot: &mut Option<(String, u16)>) {
    if matches!(slot, Some((host, port)) if host.is_empty() || *port == 0) {
        *slot = None;
    }
}

/// 查询当前系统代理并映射为环境变量；非 macOS 或查询失败时为空。
pub async fn system_proxy_env() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    {
        let output = tokio::process::Command::new("scutil")
            .arg("--proxy")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .await;
        match output {
            Ok(output) if output.status.success() => {
                parse_scutil_proxy(&String::from_utf8_lossy(&output.stdout)).to_env_pairs()
            }
            _ => Vec::new(),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// 代理环境变量同族键（大小写）。
const PROXY_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// 合并合成环境：base 优先，系统代理仅补缺，PATH 强制覆盖。
#[must_use]
pub fn build_spawn_env(
    base: EnvVars,
    proxy_pairs: &[(String, String)],
    runtime_path: OsString,
) -> EnvVars {
    let mut merged: BTreeMap<OsString, OsString> = base.into_iter().collect();
    for (key, value) in proxy_pairs {
        let family_present = PROXY_KEYS
            .iter()
            .filter(|candidate| candidate.eq_ignore_ascii_case(key))
            .any(|candidate| merged.contains_key(OsStr::new(candidate)));
        if !family_present {
            merged.insert(OsString::from(key), OsString::from(value));
        }
    }
    merged.insert(OsString::from("PATH"), runtime_path);
    merged.into_iter().collect()
}

/// 计算 Pi 子进程的运行时 PATH：继承 PATH + 常见用户级安装目录。
#[must_use]
pub fn pi_runtime_path(home: Option<&OsStr>, inherited_path: Option<&OsStr>) -> OsString {
    let mut directories: Vec<std::path::PathBuf> = inherited_path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .collect();

    if let Some(home) = home {
        let home = Path::new(home);
        directories.extend([
            home.join(".local/bin"),
            home.join(".hermes/node/bin"),
            home.join(".volta/bin"),
            home.join(".bun/bin"),
        ]);
    }
    directories.extend([
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
    ]);
    directories.dedup();

    std::env::join_paths(directories)
        .unwrap_or_else(|_| inherited_path.map_or_else(OsString::new, OsString::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用临时目录：避免新增 tempfile 依赖，进程号 + 计数保证唯一。
    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "lure-env-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn captured(entries: &[(&str, &str)], junk_before: &str) -> Vec<u8> {
        let mut output = junk_before.as_bytes().to_vec();
        output.extend_from_slice(format!("{ENV_SENTINEL}\n").as_bytes());
        for (index, (key, value)) in entries.iter().enumerate() {
            if index > 0 {
                output.push(0);
            }
            output.extend_from_slice(format!("{key}={value}").as_bytes());
        }
        output
    }

    // ---- parse_captured_env ----

    #[test]
    fn parse_captured_env_skips_rc_junk_before_sentinel() {
        let output = captured(
            &[("HOME", "/Users/demo"), ("LURE_MARKER", "hello")],
            "p10k-instant-prompt-output\n一些中文噪音\n",
        );
        let vars = parse_captured_env(&output).expect("应解析成功");
        assert!(vars.contains(&(OsString::from("HOME"), OsString::from("/Users/demo"))));
        assert!(vars.contains(&(OsString::from("LURE_MARKER"), OsString::from("hello"))));
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn parse_captured_env_returns_none_without_sentinel() {
        assert!(parse_captured_env(b"PATH=/usr/bin\0HOME=/root").is_none());
    }

    #[test]
    fn parse_captured_env_returns_none_when_no_entries() {
        let output = format!("噪音\n{ENV_SENTINEL}\n");
        assert!(parse_captured_env(output.as_bytes()).is_none());
    }

    #[test]
    fn parse_captured_env_preserves_newlines_and_equals_in_values() {
        let mut output = format!("{ENV_SENTINEL}\n").into_bytes();
        output.extend_from_slice(b"MULTI=line1\nline2=still-value");
        let vars = parse_captured_env(&output).expect("应解析成功");
        assert_eq!(
            vars,
            vec![(
                OsString::from("MULTI"),
                OsString::from("line1\nline2=still-value")
            )]
        );
    }

    #[test]
    fn parse_captured_env_skips_entries_without_equals() {
        let mut output = format!("{ENV_SENTINEL}\n").into_bytes();
        output.extend_from_slice(b"BROKEN\0OK=1");
        let vars = parse_captured_env(&output).expect("应解析成功");
        assert_eq!(vars, vec![(OsString::from("OK"), OsString::from("1"))]);
    }

    // ---- capture_login_shell_env ----

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn capture_reads_login_shell_env_and_ignores_rc_noise() {
        let temp = TestDir::new();
        std::fs::write(
            temp.0.join(".zshrc"),
            "echo rc-stdout-noise\nexport LURE_CAPTURE_MARKER=from-rc\n",
        )
        .unwrap();
        let vars = capture_login_shell_env_with(
            Some(std::path::PathBuf::from("/bin/zsh")),
            &[
                (OsString::from("ZDOTDIR"), temp.0.as_os_str().to_os_string()),
                (OsString::from("HOME"), temp.0.as_os_str().to_os_string()),
            ],
            Duration::from_secs(10),
        )
        .await
        .expect("应捕获到 shell 环境");
        assert!(vars.contains(&(
            OsString::from("LURE_CAPTURE_MARKER"),
            OsString::from("from-rc")
        )));
        assert!(!vars.iter().any(|(key, _)| key == "rc-stdout-noise"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn capture_returns_none_on_timeout() {
        let temp = TestDir::new();
        std::fs::write(temp.0.join(".zshrc"), "sleep 5\n").unwrap();
        let vars = capture_login_shell_env_with(
            Some(std::path::PathBuf::from("/bin/zsh")),
            &[
                (OsString::from("ZDOTDIR"), temp.0.as_os_str().to_os_string()),
                (OsString::from("HOME"), temp.0.as_os_str().to_os_string()),
            ],
            Duration::from_millis(300),
        )
        .await;
        assert!(vars.is_none());
    }

    // ---- parse_scutil_proxy ----

    #[test]
    fn scutil_all_disabled_yields_no_proxy() {
        let output = "<dictionary> {
  ExceptionsList : <array> {
    0 : 127.0.0.1
    1 : *.local
  }
  HTTPEnable : 0
  HTTPSEnable : 0
  SOCKSEnable : 0
}
";
        let proxy = parse_scutil_proxy(output);
        assert!(proxy.http.is_none() && proxy.https.is_none() && proxy.socks.is_none());
        // 例外列表仅在代理启用时才会映射为 NO_PROXY。
        assert!(proxy.to_env_pairs().is_empty());
    }

    #[test]
    fn scutil_enabled_proxies_map_to_env_pairs() {
        let output = "<dictionary> {
  ExceptionsList : <array> {
    0 : 127.0.0.1
    1 : 192.168.0.0/16
    2 : *.local
    3 : <local>
    4 : *.asiainfo.com
  }
  HTTPEnable : 1
  HTTPPort : 7890
  HTTPProxy : 127.0.0.1
  HTTPSEnable : 1
  HTTPSPort : 7890
  HTTPSProxy : 127.0.0.1
  SOCKSEnable : 1
  SOCKSPort : 7891
  SOCKSProxy : 127.0.0.1
}
";
        let pairs = parse_scutil_proxy(output).to_env_pairs();
        assert!(pairs.contains(&(
            "HTTP_PROXY".to_string(),
            "http://127.0.0.1:7890".to_string()
        )));
        assert!(pairs.contains(&(
            "http_proxy".to_string(),
            "http://127.0.0.1:7890".to_string()
        )));
        assert!(pairs.contains(&(
            "HTTPS_PROXY".to_string(),
            "http://127.0.0.1:7890".to_string()
        )));
        assert!(pairs.contains(&(
            "ALL_PROXY".to_string(),
            "socks5://127.0.0.1:7891".to_string()
        )));
        let no_proxy = pairs
            .iter()
            .find(|(key, _)| key == "NO_PROXY")
            .map(|(_, value)| value.clone())
            .expect("应有 NO_PROXY");
        assert_eq!(no_proxy, "127.0.0.1,192.168.0.0/16,.local,.asiainfo.com");
    }

    #[test]
    fn scutil_authenticated_proxy_is_skipped() {
        let output = "<dictionary> {
  HTTPEnable : 1
  HTTPPort : 8080
  HTTPProxy : 10.0.0.1
  HTTPProxyAuthenticated : 1
  HTTPSEnable : 0
  SOCKSEnable : 0
}
";
        assert_eq!(parse_scutil_proxy(output), SystemProxy::default());
    }

    #[test]
    fn scutil_enable_without_host_or_port_is_pruned() {
        let output = "<dictionary> {
  HTTPEnable : 1
  HTTPSEnable : 0
  SOCKSEnable : 0
}
";
        assert_eq!(parse_scutil_proxy(output), SystemProxy::default());
    }

    // ---- build_spawn_env ----

    fn vars(pairs: &[(&str, &str)]) -> EnvVars {
        pairs
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value)))
            .collect()
    }

    #[test]
    fn build_spawn_env_fills_proxy_only_when_absent() {
        let base = vars(&[
            ("HOME", "/Users/demo"),
            ("https_proxy", "http://rc-proxy:1"),
        ]);
        let proxy = vec![
            ("HTTP_PROXY".to_string(), "http://sys:2".to_string()),
            ("HTTPS_PROXY".to_string(), "http://sys:2".to_string()),
            ("https_proxy".to_string(), "http://sys:2".to_string()),
            ("NO_PROXY".to_string(), ".local".to_string()),
        ];
        let merged = build_spawn_env(base, &proxy, OsString::from("/usr/bin"));
        assert!(merged.contains(&(
            OsString::from("https_proxy"),
            OsString::from("http://rc-proxy:1")
        )));
        // shell 环境已有 https_proxy（小写）→ 整个 https 族不补
        assert!(!merged.contains(&(
            OsString::from("HTTPS_PROXY"),
            OsString::from("http://sys:2")
        )));
        // 没有的键正常补
        assert!(merged.contains(&(OsString::from("HTTP_PROXY"), OsString::from("http://sys:2"))));
        assert!(merged.contains(&(OsString::from("NO_PROXY"), OsString::from(".local"))));
    }

    #[test]
    fn build_spawn_env_forces_runtime_path() {
        let base = vars(&[("PATH", "/original/bin")]);
        let merged = build_spawn_env(base, &[], OsString::from("/runtime/bin"));
        assert!(merged.contains(&(OsString::from("PATH"), OsString::from("/runtime/bin"))));
        assert!(!merged.contains(&(OsString::from("PATH"), OsString::from("/original/bin"))));
    }

    // ---- SpawnEnvResolver 冒烟 ----

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn resolve_returns_env_with_runtime_path() {
        let resolver = SpawnEnvResolver::new();
        let env = resolver.resolve().await;
        let path = env
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone())
            .expect("合成环境必须有 PATH");
        let path = path.to_string_lossy();
        assert!(path.contains("/opt/homebrew/bin") || path.contains("/usr/local/bin"));
    }
}
