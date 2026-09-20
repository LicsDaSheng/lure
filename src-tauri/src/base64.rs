//! 最小 base64 编码，用于把本地图片作为 RPC 附件传输。

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 将字节编码为标准 base64（带 `=` 填充）。
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk.first().copied().unwrap_or_default());
        let second = u32::from(chunk.get(1).copied().unwrap_or_default());
        let third = u32::from(chunk.get(2).copied().unwrap_or_default());
        let triple = (first << 16) | (second << 8) | third;

        output.push(char::from(ALPHABET[((triple >> 18) & 0x3f) as usize]));
        output.push(char::from(ALPHABET[((triple >> 12) & 0x3f) as usize]));
        output.push(if chunk.len() > 1 {
            char::from(ALPHABET[((triple >> 6) & 0x3f) as usize])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(ALPHABET[(triple & 0x3f) as usize])
        } else {
            '='
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_padding_boundaries() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"hello"), "aGVsbG8=");
    }
}
