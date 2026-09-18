use lure_rpc::{RpcError, jsonl::read_json_lines};
use serde_json::json;
use tokio::io::{AsyncWriteExt, duplex};
use tokio::sync::mpsc;

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
