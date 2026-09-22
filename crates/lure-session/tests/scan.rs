use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lure_session::{
    SessionSummary, agent_directory, list_sessions, list_sessions_page, read_session_summary,
    session_directory,
};
use serde_json::{Value, json};

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "lure-session-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("project")).unwrap();
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    fn agent(&self) -> PathBuf {
        self.root.join("agent")
    }

    fn sessions_dir(&self) -> PathBuf {
        session_directory(&self.project(), &self.agent())
    }

    /// 在默认会话目录写入一个会话文件，返回其路径。
    fn write_session(&self, file_name: &str, lines: &[Value]) -> PathBuf {
        let directory = self.sessions_dir();
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(file_name);
        let body: String = lines.iter().fold(String::new(), |mut body, line| {
            let _ = writeln!(body, "{line}");
            body
        });
        std::fs::write(&path, body).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn header(id: &str, timestamp: &str, cwd: &str) -> Value {
    json!({ "type": "session", "version": 3, "id": id, "timestamp": timestamp, "cwd": cwd })
}

fn user_message(id: &str, timestamp_ms: i64, text: &str) -> Value {
    json!({
        "type": "message",
        "id": id,
        "parentId": Value::Null,
        "timestamp": "2026-09-20T10:00:00.000Z",
        "message": { "role": "user", "content": text, "timestamp": timestamp_ms }
    })
}

fn assistant_message(id: &str, timestamp_ms: i64, text: &str) -> Value {
    json!({
        "type": "message",
        "id": id,
        "parentId": Value::Null,
        "timestamp": "2026-09-20T10:00:01.000Z",
        "message": {
            "role": "assistant",
            "content": [{ "type": "text", "text": text }],
            "timestamp": timestamp_ms
        }
    })
}

fn names_of(sessions: &[SessionSummary]) -> Vec<String> {
    sessions.iter().map(|session| session.id.clone()).collect()
}

#[tokio::test]
async fn lists_a_working_directory_newest_first_with_full_metadata() {
    let fixture = Fixture::new("list");
    let cwd = fixture.project();
    let cwd_text = cwd.to_string_lossy().to_string();

    fixture.write_session(
        "2026-09-20T10-00-00-000Z_older.jsonl",
        &[
            header("older", "2026-09-20T10:00:00.000Z", &cwd_text),
            user_message("u1", 1_788_054_000_000, "先看项目结构"),
            assistant_message("a1", 1_788_054_010_000, "好的"),
        ],
    );
    fixture.write_session(
        "2026-09-20T11-00-00-000Z_newer.jsonl",
        &[
            header("newer", "2026-09-20T11:00:00.000Z", &cwd_text),
            json!({
                "type": "session_info",
                "id": "i1",
                "parentId": Value::Null,
                "timestamp": "2026-09-20T11:00:05.000Z",
                "name": "重构任务"
            }),
            user_message("u2", 1_788_057_600_000, "帮我重构会话列表"),
            assistant_message("a2", 1_788_057_610_000, "开始"),
        ],
    );

    let sessions = list_sessions(&cwd, &fixture.agent()).await.unwrap();

    assert_eq!(names_of(&sessions), vec!["newer", "older"]);
    let newest = &sessions[0];
    assert_eq!(newest.name.as_deref(), Some("重构任务"));
    assert_eq!(newest.first_message.as_deref(), Some("帮我重构会话列表"));
    assert_eq!(newest.message_count, 2);
    assert_eq!(newest.cwd.as_deref(), Some(cwd_text.as_str()));
    assert_eq!(newest.created_at_ms, 1_789_902_000_000);
    assert_eq!(newest.modified_at_ms, 1_788_057_610_000);
    assert_eq!(newest.parent_session_path, None);
    assert!(newest.path.starts_with(fixture.sessions_dir()));

    let oldest = &sessions[1];
    assert_eq!(oldest.name, None);
    assert_eq!(oldest.modified_at_ms, 1_788_054_010_000);
}

#[tokio::test]
async fn pages_sessions_on_the_backend_and_reports_more_records() {
    let fixture = Fixture::new("page");
    let cwd = fixture.project();
    let cwd_text = cwd.to_string_lossy().to_string();
    for index in 0..9 {
        fixture.write_session(
            &format!("{index}.jsonl"),
            &[
                header(
                    &format!("session-{index}"),
                    "2026-09-20T10:00:00.000Z",
                    &cwd_text,
                ),
                user_message(
                    &format!("message-{index}"),
                    1_788_054_000_000 + index,
                    &format!("会话 {index}"),
                ),
            ],
        );
    }

    let first = list_sessions_page(&cwd, &fixture.agent(), 0, 3)
        .await
        .unwrap();
    assert_eq!(
        names_of(&first.sessions),
        vec!["session-8", "session-7", "session-6"]
    );
    assert!(first.has_more);

    let second = list_sessions_page(&cwd, &fixture.agent(), 3, 5)
        .await
        .unwrap();
    assert_eq!(
        names_of(&second.sessions),
        vec![
            "session-5",
            "session-4",
            "session-3",
            "session-2",
            "session-1"
        ]
    );
    assert!(second.has_more);

    let last = list_sessions_page(&cwd, &fixture.agent(), 8, 5)
        .await
        .unwrap();
    assert_eq!(names_of(&last.sessions), vec!["session-0"]);
    assert!(!last.has_more);
}

#[tokio::test]
async fn keeps_the_latest_session_name_including_clears() {
    let fixture = Fixture::new("name");
    let cwd_text = fixture.project().to_string_lossy().to_string();
    fixture.write_session(
        "named.jsonl",
        &[
            header("named", "2026-09-20T10:00:00.000Z", &cwd_text),
            json!({
                "type": "session_info",
                "id": "i1",
                "parentId": Value::Null,
                "timestamp": "2026-09-20T10:00:01.000Z",
                "name": "旧名字"
            }),
            json!({
                "type": "session_info",
                "id": "i2",
                "parentId": "i1",
                "timestamp": "2026-09-20T10:00:02.000Z",
                "name": "   "
            }),
        ],
    );

    let sessions = list_sessions(&fixture.project(), &fixture.agent())
        .await
        .unwrap();

    assert_eq!(sessions[0].name, None);
}

#[tokio::test]
async fn falls_back_to_the_header_timestamp_when_a_session_has_no_messages() {
    let fixture = Fixture::new("empty");
    let cwd_text = fixture.project().to_string_lossy().to_string();
    fixture.write_session(
        "empty.jsonl",
        &[header("empty", "2026-09-20T10:00:00.000Z", &cwd_text)],
    );

    let sessions = list_sessions(&fixture.project(), &fixture.agent())
        .await
        .unwrap();

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].modified_at_ms, 1_789_898_400_000);
    assert_eq!(sessions[0].message_count, 0);
    assert_eq!(sessions[0].first_message, None);
}

#[tokio::test]
async fn skips_malformed_lines_and_files_without_a_session_header() {
    let fixture = Fixture::new("invalid");
    let cwd_text = fixture.project().to_string_lossy().to_string();
    let directory = fixture.sessions_dir();
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("broken.jsonl"),
        format!("{{\"type\":\"session\"\n{}\n", json!({"oops": true})),
    )
    .unwrap();
    std::fs::write(
        directory.join("history-only.jsonl"),
        format!("{}\n", user_message("u1", 1, "没有会话头")),
    )
    .unwrap();
    std::fs::write(directory.join("notes.txt"), "不是会话文件\n").unwrap();
    fixture.write_session(
        "valid.jsonl",
        &[
            header("valid", "2026-09-20T10:00:00.000Z", &cwd_text),
            user_message("u1", 1_788_055_300_000, "有效会话"),
        ],
    );

    let sessions = list_sessions(&fixture.project(), &fixture.agent())
        .await
        .unwrap();

    assert_eq!(names_of(&sessions), vec!["valid"]);
}

#[tokio::test]
async fn returns_an_empty_list_when_no_session_directory_exists() {
    let fixture = Fixture::new("missing");

    let sessions = list_sessions(&fixture.project(), &fixture.agent())
        .await
        .unwrap();

    assert!(sessions.is_empty());
}

#[tokio::test]
async fn falls_back_to_scanning_all_projects_when_the_encoded_directory_is_missing() {
    let fixture = Fixture::new("fallback");
    let other_project = fixture.root.join("other");
    std::fs::create_dir_all(&other_project).unwrap();
    let cwd_text = fixture.project().to_string_lossy().to_string();

    // 会话被记录在另一个目录名下（例如 Pi 曾以非规范化路径启动），但仍声明当前工作目录。
    let other_dir = session_directory(&other_project, &fixture.agent());
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(
        other_dir.join("moved.jsonl"),
        format!(
            "{}\n",
            header("moved", "2026-09-20T10:00:00.000Z", &cwd_text)
        ),
    )
    .unwrap();
    std::fs::write(
        other_dir.join("elsewhere.jsonl"),
        format!(
            "{}\n",
            header(
                "elsewhere",
                "2026-09-20T10:00:00.000Z",
                "/tmp/other-project"
            )
        ),
    )
    .unwrap();

    let sessions = list_sessions(&fixture.project(), &fixture.agent())
        .await
        .unwrap();

    assert_eq!(names_of(&sessions), vec!["moved"]);
}

#[tokio::test]
async fn reads_a_single_session_summary_and_rejects_foreign_files() {
    let fixture = Fixture::new("single");
    let cwd_text = fixture.project().to_string_lossy().to_string();
    let path = fixture.write_session(
        "single.jsonl",
        &[
            header("single", "2026-09-20T10:00:00.000Z", &cwd_text),
            user_message("u1", 1_788_055_300_000, "只有一条"),
        ],
    );

    let summary = read_session_summary(&path).await.unwrap().unwrap();
    assert_eq!(summary.id, "single");
    assert_eq!(summary.message_count, 1);

    let notes = fixture.root.join("notes.txt");
    std::fs::write(&notes, "不是会话\n").unwrap();
    assert_eq!(read_session_summary(&notes).await.unwrap(), None);

    // 文件缺失与内容无效都表现为“无法打开该会话”，调用方无需区分。
    assert_eq!(
        read_session_summary(&fixture.root.join("absent.jsonl"))
            .await
            .unwrap(),
        None
    );
}

#[test]
fn resolves_the_agent_directory_from_the_home_directory() {
    let agent = agent_directory(Path::new("/Users/me"));

    assert!(agent.ends_with(Path::new(".pi").join("agent")));
}
