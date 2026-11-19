//! 项目切换弹框的选择状态，不涉及 Pi 会话内部状态。

gpui::actions!(project_picker, [NextProject, PreviousProject]);

pub(crate) fn key_bindings() -> Vec<gpui::KeyBinding> {
    vec![
        gpui::KeyBinding::new("secondary-j", NextProject, Some("ProjectPicker")),
        gpui::KeyBinding::new("secondary-k", PreviousProject, Some("ProjectPicker")),
    ]
}

#[derive(Default)]
pub(crate) struct ProjectPicker {
    pub(crate) selected: Option<usize>,
}

impl ProjectPicker {
    pub(crate) fn new(count: usize, active: Option<usize>) -> Self {
        Self {
            selected: active
                .filter(|&index| index < count)
                .or((count > 0).then_some(0)),
        }
    }

    pub(crate) fn move_selection(&mut self, count: usize, down: bool) {
        self.selected = count.checked_sub(1).map(|last| {
            let current = self.selected.unwrap_or(0).min(last);
            if down {
                current.saturating_add(1).min(last)
            } else {
                current.saturating_sub(1)
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_navigation_uses_registered_actions() {
        let bindings = key_bindings();
        let mut picker = ProjectPicker::new(3, Some(1));
        for (key, expected) in [("secondary-j", 2), ("secondary-k", 1)] {
            let keystroke = gpui::Keystroke::parse(key).unwrap();
            let binding = bindings
                .iter()
                .find(|binding| {
                    binding.match_keystrokes(std::slice::from_ref(&keystroke)) == Some(false)
                })
                .expect("项目导航快捷键必须绑定动作");
            assert!(binding.predicate().is_some(), "导航必须限定在弹框上下文");
            picker.move_selection(3, binding.action().as_any().is::<NextProject>());
            assert_eq!(picker.selected, Some(expected));
        }
        for key in ["j", "k", "secondary-e"] {
            let keystroke = gpui::Keystroke::parse(key).unwrap();
            assert!(bindings.iter().all(|binding| {
                binding
                    .match_keystrokes(std::slice::from_ref(&keystroke))
                    .is_none()
            }));
        }
    }

    #[test]
    fn starts_at_active_project_or_first_entry() {
        assert_eq!(ProjectPicker::new(3, Some(2)).selected, Some(2));
        assert_eq!(ProjectPicker::new(3, None).selected, Some(0));
        assert_eq!(ProjectPicker::new(3, Some(9)).selected, Some(0));
    }

    #[test]
    fn navigation_stops_at_list_boundaries() {
        let mut picker = ProjectPicker::new(3, None);
        picker.move_selection(3, false);
        assert_eq!(picker.selected, Some(0));
        picker.move_selection(3, true);
        assert_eq!(picker.selected, Some(1));
        picker.move_selection(3, true);
        picker.move_selection(3, true);
        assert_eq!(picker.selected, Some(2));
        picker.move_selection(3, false);
        assert_eq!(picker.selected, Some(1));
    }

    #[test]
    fn empty_list_has_no_selection() {
        let mut picker = ProjectPicker::new(0, Some(0));
        picker.move_selection(0, true);
        picker.move_selection(0, false);
        assert_eq!(picker.selected, None);
    }
}
