//! 桌面窗口的逻辑像素布局与本地记忆，不涉及 Pi 配置。
use gpui::{Bounds, Pixels, point, px, size};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct Geometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Geometry {
    fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 0.
            && self.height > 0.
    }
}

pub(crate) fn fit(saved: Option<Geometry>, area: Bounds<Pixels>) -> Bounds<Pixels> {
    let saved = saved.filter(|g| g.valid());
    let aw = f32::from(area.size.width);
    let ah = f32::from(area.size.height);
    let width = saved
        .map_or(1200_f32.min(aw * 0.9), |g| g.width)
        .max(720.)
        .min(aw);
    let height = saved
        .map_or(800_f32.min(ah * 0.9), |g| g.height)
        .max(480.)
        .min(ah);
    let left = f32::from(area.origin.x);
    let top = f32::from(area.origin.y);
    let x = saved
        .map_or(left + (aw - width) / 2., |g| g.x)
        .clamp(left, left + aw - width);
    let y = saved
        .map_or(top + (ah - height) / 2., |g| g.y)
        .clamp(top, top + ah - height);
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn state_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/lure/window.yaml"))
}

pub(crate) fn load() -> Option<Geometry> {
    let contents = std::fs::read_to_string(state_path()?).ok()?;
    serde_yaml::from_str::<Geometry>(&contents)
        .ok()
        .filter(|g| g.valid())
}

pub(crate) fn startup(cx: &gpui::App) -> Bounds<Pixels> {
    let saved = load();
    let displays = cx.displays();
    let display = saved
        .and_then(|g| {
            displays
                .iter()
                .find(|d| {
                    d.visible_bounds()
                        .contains(&point(px(g.x + g.width / 2.), px(g.y + g.height / 2.)))
                })
                .cloned()
        })
        .or_else(|| cx.primary_display());
    let area = display.map_or_else(
        || Bounds::new(point(px(0.), px(0.)), size(px(1440.), px(900.))),
        |d| d.visible_bounds(),
    );
    fit(saved, area)
}

pub(crate) fn save(bounds: Bounds<Pixels>) -> Result<(), String> {
    let Some(path) = state_path() else {
        return Ok(());
    };
    let geometry = Geometry {
        x: bounds.origin.x.into(),
        y: bounds.origin.y.into(),
        width: bounds.size.width.into(),
        height: bounds.size.height.into(),
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let contents = serde_yaml::to_string(&geometry).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, contents.as_bytes()).map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)))
    }

    #[test]
    fn default_is_centered() {
        let b = fit(None, area(1920., 1080.));
        assert_eq!(b.size, size(px(1200.), px(800.)));
        assert_eq!(b.origin, point(px(360.), px(140.)));
    }

    #[test]
    fn small_screen_uses_ninety_percent() {
        let b = fit(None, area(1280., 720.));
        assert_eq!(b.size, size(px(1152.), px(648.)));
    }

    #[test]
    fn restored_size_is_not_limited_to_default() {
        let saved = Geometry {
            x: 20.,
            y: 30.,
            width: 1500.,
            height: 900.,
        };
        let b = fit(Some(saved), area(1920., 1080.));
        assert_eq!(b.origin, point(px(20.), px(30.)));
        assert_eq!(b.size, size(px(1500.), px(900.)));
    }

    #[test]
    fn disconnected_display_is_clamped() {
        let saved = Geometry {
            x: -2000.,
            y: 2000.,
            width: 2000.,
            height: 1200.,
        };
        let b = fit(Some(saved), area(1280., 720.));
        assert_eq!(b, area(1280., 720.));
    }

    #[test]
    fn minimum_and_tiny_work_areas() {
        assert_eq!(fit(None, area(750., 500.)).size, size(px(720.), px(480.)));
        assert_eq!(fit(None, area(640., 400.)), area(640., 400.));
    }

    #[test]
    fn work_area_origin_is_respected() {
        let area = Bounds::new(point(px(-1920.), px(25.)), size(px(1920.), px(1055.)));
        let b = fit(None, area);
        assert_eq!(b.origin, point(px(-1560.), px(152.5)));
    }

    #[test]
    fn geometry_yaml_round_trip() {
        let saved = Geometry {
            x: -900.,
            y: 25.,
            width: 1200.,
            height: 800.,
        };
        let yaml = serde_yaml::to_string(&saved).unwrap();
        let restored: Geometry = serde_yaml::from_str(&yaml).unwrap();
        assert!(restored.valid());
        assert!((restored.x - saved.x).abs() < f32::EPSILON);
        assert!((restored.width - saved.width).abs() < f32::EPSILON);
        assert!(serde_yaml::from_str::<Geometry>("broken: [").is_err());
    }

    #[test]
    fn invalid_geometry_falls_back() {
        let saved = Geometry {
            x: f32::NAN,
            y: 0.,
            width: -1.,
            height: 0.,
        };
        assert_eq!(
            fit(Some(saved), area(1920., 1080.)),
            fit(None, area(1920., 1080.))
        );
    }
}
