//! 手动深浅主题、界面语义配色与终端 ANSI 色板。

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

/// 应用主题仅支持手动深色和浅色，不订阅系统外观。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    Dark,
    #[default]
    Light,
}

/// 原生界面的语义颜色，所有页面共用同一主题。
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: u32,
    pub titlebar: u32,
    pub foreground: u32,
    pub muted: u32,
    pub border: u32,
    pub error: u32,
    pub accent: u32,
    pub home_background: u32,
    pub sidebar: u32,
    pub home_titlebar: u32,
    pub card: u32,
    pub home_border: u32,
    pub surface: u32,
    pub selected: u32,
    pub badge: u32,
    pub secondary: u32,
    pub hint: u32,
    pub success: u32,
    pub status_working: u32,
    pub status_running: u32,
    pub status_idle: u32,
    pub success_border: u32,
    pub primary: u32,
    pub primary_hover: u32,
    pub on_primary: u32,
    pub brand: u32,
    pub on_brand: u32,
    pub path: u32,
}

impl Theme {
    #[must_use]
    pub fn toggle(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    #[must_use]
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                background: BACKGROUND,
                titlebar: TITLEBAR,
                foreground: FOREGROUND,
                muted: MUTED,
                border: BORDER,
                error: ERROR,
                accent: ACCENT,
                home_background: 0x0009_0B0D,
                sidebar: 0x000E_1014,
                home_titlebar: 0x000E_1014,
                card: 0x0010_1419,
                home_border: 0x0024_2A30,
                surface: 0x0015_1920,
                selected: 0x001D_2026,
                badge: 0x0030_343B,
                secondary: 0x00A8_B6C7,
                hint: 0x0074_899E,
                success: 0x002A_D7A5,
                status_working: 0x00FF_5555,
                status_running: 0x004A_DE80,
                status_idle: 0x0052_5252,
                success_border: 0x001B_5145,
                primary: 0x00EE_F2F5,
                primary_hover: 0x00D8_E0E8,
                on_primary: 0x0010_1419,
                brand: 0x0016_C7B5,
                on_brand: 0x0009_211F,
                path: 0x000B_0E12,
            },
            Self::Light => Palette {
                background: 0x00F7_F8FA,
                titlebar: 0x00F7_F8FA,
                foreground: 0x000F_172A,
                muted: 0x0064_748B,
                border: 0x00D8_DCE3,
                error: 0x00B9_1C1C,
                accent: 0x004F_46E5,
                home_background: 0x00F7_F8FA,
                sidebar: 0x00F0_F2F5,
                home_titlebar: 0x00F7_F8FA,
                card: 0x00FF_FFFF,
                home_border: 0x00E2_E5EC,
                surface: 0x00FF_FFFF,
                selected: 0x00E0_E7FF,
                badge: 0x00EE_F2FF,
                secondary: 0x0047_5569,
                hint: 0x0064_748B,
                success: 0x0004_7857,
                status_working: 0x00DC_2626,
                status_running: 0x0016_A34A,
                status_idle: 0x00A3_A3A3,
                success_border: 0x00A7_F3D0,
                primary: 0x004F_46E5,
                primary_hover: 0x0043_38CA,
                on_primary: 0x00FF_FFFF,
                brand: 0x004F_46E5,
                on_brand: 0x00FF_FFFF,
                path: 0x00F8_FAFC,
            },
        }
    }

    #[must_use]
    pub fn indexed_color(self, index: u8) -> u32 {
        const LIGHT: [u32; 16] = [
            0x001E_293B,
            0x00B9_1C1C,
            0x0004_7857,
            0x0092_400E,
            0x0043_38CA,
            0x00A2_1CAF,
            0x000E_7490,
            0x00CB_D5E1,
            0x0064_748B,
            0x00DC_2626,
            0x0015_803D,
            0x00A1_6207,
            0x004F_46E5,
            0x00C0_26D3,
            0x0008_91B2,
            0x00FF_FFFF,
        ];
        if self == Self::Light && index < 16 {
            LIGHT[usize::from(index)]
        } else {
            indexed_color(index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_has_only_two_manual_states() {
        assert_eq!(Theme::default(), Theme::Light);
        assert_eq!(Theme::Dark.toggle(), Theme::Light);
        assert_eq!(Theme::Light.toggle(), Theme::Dark);
    }

    #[test]
    fn light_palette_matches_reference() {
        let p = Theme::Light.palette();
        assert_eq!(p.home_background, 0x00F7_F8FA);
        assert_eq!(p.sidebar, 0x00F0_F2F5);
        assert_eq!(p.card, 0x00FF_FFFF);
        assert_eq!(p.accent, 0x004F_46E5);
        assert_ne!(p.foreground, p.background);
        assert_eq!(Theme::Dark.palette().background, BACKGROUND);
    }

    #[test]
    fn extended_ansi_palette_is_not_rewritten() {
        for index in 16..=255 {
            assert_eq!(Theme::Light.indexed_color(index), indexed_color(index));
        }
        assert_ne!(Theme::Light.indexed_color(1), Theme::Dark.indexed_color(1));
    }
}

#[cfg(test)]
mod status_tests {
    use super::Theme;

    #[test]
    fn project_status_uses_red_green_and_neutral_gray() {
        assert_eq!(Theme::Dark.palette().status_working, 0x00FF_5555);
        assert_eq!(Theme::Light.palette().status_working, 0x00DC_2626);
        assert_eq!(Theme::Dark.palette().status_running, 0x004A_DE80);
        assert_eq!(Theme::Light.palette().status_running, 0x0016_A34A);
        assert_eq!(Theme::Dark.palette().status_idle, 0x0052_5252);
        assert_eq!(Theme::Light.palette().status_idle, 0x00A3_A3A3);

        for theme in [Theme::Dark, Theme::Light] {
            let palette = theme.palette();
            let working = palette.status_working;
            let running = palette.status_running;
            let idle = palette.status_idle;
            let working_red = (working >> 16) & 255;
            let working_green = (working >> 8) & 255;
            let working_blue = working & 255;
            assert!(working_red > working_green + 80 && working_red > working_blue + 80);
            let red = (running >> 16) & 255;
            let green = (running >> 8) & 255;
            let blue = running & 255;
            assert!(green > red + 80 && green > blue + 40);
            assert_eq!((idle >> 16) & 255, (idle >> 8) & 255);
            assert_eq!((idle >> 8) & 255, idle & 255);
            assert_ne!(working, running);
            assert_ne!(working, idle);
            assert_ne!(running, idle);
        }
    }
}
