use lure_rpc::{
    RpcError,
    capture::StdoutCapture,
    jsonl::{read_json_lines, read_json_lines_with_capture},
};
use serde_json::json;
use tokio::io::{AsyncWriteExt, duplex};
use tokio::sync::mpsc;

async fn capture_file(name: &str) -> (StdoutCapture, std::path::PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("lure-capture-{}-{name}", std::process::id()));
    let _ = tokio::fs::remove_dir_all(&directory).await;
    let path = directory.join("stdout.jsonl");
    let capture = StdoutCapture::open(&path).await.unwrap();
    (capture, path)
}

#[tokio::test]
async fn capture_records_every_raw_line_before_parsing() {
    let (mut capture, path) = capture_file("raw").await;
    let (sender, mut receiver) = mpsc::unbounded_channel();

    let result = read_json_lines_with_capture(
        &b"{\"type\":\"first\"}\r\nnot-json\n"[..],
        1024,
        sender,
        Some(&mut capture),
    )
    .await;

    assert!(matches!(result, Err(RpcError::InvalidJson(_))));
    assert_eq!(receiver.recv().await.unwrap(), json!({"type":"first"}));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"{\"type\":\"first\"}\r\nnot-json\n",
        "采集必须保留原始行字节，包括 CRLF 与解析失败的行"
    );
    drop(capture);
    tokio::fs::remove_dir_all(path.parent().unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_terminates_a_line_missing_its_newline() {
    let (mut capture, path) = capture_file("trailing").await;
    let (sender, _receiver) = mpsc::unbounded_channel();

    read_json_lines_with_capture(
        &b"{\"type\":\"only\"}"[..],
        1024,
        sender,
        Some(&mut capture),
    )
    .await
    .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"{\"type\":\"only\"}\n");
    drop(capture);
    tokio::fs::remove_dir_all(path.parent().unwrap())
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_writes_nothing_when_it_is_disabled() {
    let (sender, mut receiver) = mpsc::unbounded_channel();

    read_json_lines_with_capture(&b"{\"type\":\"first\"}\n"[..], 1024, sender, None)
        .await
        .unwrap();

    assert_eq!(receiver.recv().await.unwrap(), json!({"type":"first"}));
    assert!(receiver.recv().await.is_none());
}

#[tokio::test]
async fn reads_fragmented_lf_delimited_json_records() {
    let (mut writer, reader) = duplex(128);
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(read_json_lines(reader, 1024, sender));

    writer.write_all(b"{\"type\":\"fir").await.unwrap();
    writer
        .write_all(b"st\",\"text\":\"a\"}\n{\"type\":\"second\"}\r\n")
        .await
        .unwrap();
    drop(writer);

    assert_eq!(
        receiver.recv().await.unwrap(),
        json!({"type":"first","text":"a"})
    );
    assert_eq!(receiver.recv().await.unwrap(), json!({"type":"second"}));
    assert!(receiver.recv().await.is_none());
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn unicode_line_separators_remain_inside_json_strings() {
    let (mut writer, reader) = duplex(128);
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(read_json_lines(reader, 1024, sender));

    writer
        .write_all("{\"text\":\"第一行\u{2028}第二行\u{2029}第三行\"}\n".as_bytes())
        .await
        .unwrap();
    drop(writer);

    assert_eq!(
        receiver.recv().await.unwrap()["text"],
        "第一行\u{2028}第二行\u{2029}第三行"
    );
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn rejects_invalid_utf8() {
    let (mut writer, reader) = duplex(64);
    let (sender, _receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(read_json_lines(reader, 1024, sender));

    writer.write_all(&[0xff, b'\n']).await.unwrap();
    drop(writer);

    assert!(matches!(task.await.unwrap(), Err(RpcError::InvalidUtf8(_))));
}

#[tokio::test]
async fn rejects_invalid_json() {
    let (mut writer, reader) = duplex(64);
    let (sender, _receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(read_json_lines(reader, 1024, sender));

    writer.write_all(b"not-json\n").await.unwrap();
    drop(writer);

    assert!(matches!(task.await.unwrap(), Err(RpcError::InvalidJson(_))));
}

#[tokio::test]
async fn rejects_frames_over_the_configured_limit() {
    let (mut writer, reader) = duplex(64);
    let (sender, _receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(read_json_lines(reader, 8, sender));

    writer.write_all(b"{\"value\":123}\n").await.unwrap();
    drop(writer);

    assert!(matches!(
        task.await.unwrap(),
        Err(RpcError::FrameTooLarge { limit: 8 })
    ));
}
