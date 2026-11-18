//! Dracula 深色界面与终端配色。

pub const BACKGROUND: u32 = 0x0028_2A36;
pub const TITLEBAR: u32 = 0x0021_222C;
pub const FOREGROUND: u32 = 0x00F8_F8F2;
pub const MUTED: u32 = 0x00A6_ABCB;
pub const BORDER: u32 = 0x0044_475A;
pub const ERROR: u32 = 0x00FF_5555;
pub const ACCENT: u32 = 0x00BD_93F9;

pub const ANSI_COLORS: [u32; 16] = [
    0x0021_222C,
    0x00FF_5555,
    0x0050_FA7B,
    0x00F1_FA8C,
    0x00BD_93F9,
    0x00FF_79C6,
    0x008B_E9FD,
    0x00F8_F8F2,
    0x0062_72A4,
    0x00FF_6E6E,
    0x0069_FF94,
    0x00FF_FFA5,
    0x00D6_ACFF,
    0x00FF_92DF,
    0x00A4_FFFF,
    0x00FF_FFFF,
];

#[must_use]
pub fn indexed_color(index: u8) -> u32 {
    if index < 16 {
        return ANSI_COLORS[usize::from(index)];
    }
    if index < 232 {
        let value = index - 16;
        let component = |slot: u8| -> u32 {
            if slot == 0 {
                0
            } else {
                55 + u32::from(slot) * 40
            }
        };
        let red = component(value / 36);
        let green = component((value / 6) % 6);
        let blue = component(value % 6);
        return (red << 16) | (green << 8) | blue;
    }
    let gray = 8 + u32::from(index - 232) * 10;
    (gray << 16) | (gray << 8) | gray
}
