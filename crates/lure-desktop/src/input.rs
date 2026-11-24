//! 将 GPUI 按键转换为 xterm 兼容字节序列。

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInput {
    pub key: String,
    pub key_char: Option<String>,
    pub modifiers: Modifiers,
}

impl KeyInput {
    #[must_use]
    pub fn new(key: impl Into<String>, key_char: Option<&str>, modifiers: Modifiers) -> Self {
        Self {
            key: key.into(),
            key_char: key_char.map(ToOwned::to_owned),
            modifiers,
        }
    }

    #[must_use]
    pub fn plain(key: impl Into<String>, key_char: Option<&str>) -> Self {
        Self::new(key, key_char, Modifiers::default())
    }
}

#[must_use]
pub fn terminal_input(input: &KeyInput) -> Option<Vec<u8>> {
    let key = input.key.to_ascii_lowercase();
    if input.modifiers.platform {
        return None;
    }
    if input.modifiers.control && !input.modifiers.alt && !input.modifiers.shift {
        let command = match key.as_str() {
            "n" => Some("/new\r"),
            "r" => Some("/resume\r"),
            _ => None,
        };
        if let Some(command) = command {
            return Some(command.as_bytes().to_vec());
        }
    }
    if input.modifiers.control
        && let Some(byte) = control_byte(&key)
    {
        return Some(vec![byte]);
    }

    let sequence = match key.as_str() {
        "enter" => Some("\r"),
        "backspace" => Some("\x7f"),
        "tab" if input.modifiers.shift => Some("\x1b[Z"),
        "tab" => Some("\t"),
        "escape" => Some("\x1b"),
        "up" => Some("\x1b[A"),
        "down" => Some("\x1b[B"),
        "right" => Some("\x1b[C"),
        "left" => Some("\x1b[D"),
        "home" => Some("\x1b[H"),
        "end" => Some("\x1b[F"),
        "pageup" => Some("\x1b[5~"),
        "pagedown" => Some("\x1b[6~"),
        "delete" => Some("\x1b[3~"),
        "insert" => Some("\x1b[2~"),
        "f1" => Some("\x1bOP"),
        "f2" => Some("\x1bOQ"),
        "f3" => Some("\x1bOR"),
        "f4" => Some("\x1bOS"),
        "f5" => Some("\x1b[15~"),
        "f6" => Some("\x1b[17~"),
        "f7" => Some("\x1b[18~"),
        "f8" => Some("\x1b[19~"),
        "f9" => Some("\x1b[20~"),
        "f10" => Some("\x1b[21~"),
        "f11" => Some("\x1b[23~"),
        "f12" => Some("\x1b[24~"),
        _ => None,
    };
    if let Some(sequence) = sequence {
        return Some(sequence.as_bytes().to_vec());
    }

    input.key_char.as_ref().map(|text| {
        let mut bytes = Vec::with_capacity(text.len() + usize::from(input.modifiers.alt));
        if input.modifiers.alt {
            bytes.push(0x1b);
        }
        bytes.extend_from_slice(text.as_bytes());
        bytes
    })
}

fn control_byte(key: &str) -> Option<u8> {
    let byte = key.as_bytes().first().copied()?;
    if key.len() == 1 && byte.is_ascii_alphabetic() {
        return Some(byte.to_ascii_uppercase() - b'@');
    }
    match key {
        "space" | "@" => Some(0),
        "[" => Some(27),
        "\\" => Some(28),
        "]" => Some(29),
        "^" => Some(30),
        "_" => Some(31),
        _ => None,
    }
}
