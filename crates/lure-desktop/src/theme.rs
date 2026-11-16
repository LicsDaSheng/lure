//! Gruvbox Light 终端配色。

pub const BACKGROUND: u32 = 0x00FB_F1C7;
pub const TITLEBAR: u32 = 0x00EB_DBB2;
pub const FOREGROUND: u32 = 0x003C_3836;
pub const MUTED: u32 = 0x007C_6F64;
pub const BORDER: u32 = 0x00D5_C4A1;
pub const ERROR: u32 = 0x00CC_241D;
pub const ACCENT: u32 = 0x0045_8588;

pub const ANSI_COLORS: [u32; 16] = [
    BACKGROUND,
    0x00CC_241D,
    0x0098_971A,
    0x00D7_9921,
    0x0045_8588,
    0x00B1_6286,
    0x0068_9D6A,
    MUTED,
    0x0092_8374,
    0x009D_0006,
    0x0079_740E,
    0x00B5_7614,
    0x0007_6678,
    0x008F_3F71,
    0x0042_7B58,
    FOREGROUND,
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
