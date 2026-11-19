//! 首页项目搜索与会话状态过滤，不解析 Pi 终端输出。

use crate::projects::ProjectEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectFilter {
    All,
    Running,
    Idle,
}

#[must_use]
pub fn matches_project(
    project: &ProjectEntry,
    query: &str,
    running: bool,
    filter: ProjectFilter,
) -> bool {
    let state_matches = match filter {
        ProjectFilter::All => true,
        ProjectFilter::Running => running,
        ProjectFilter::Idle => !running,
    };
    let query = query.trim().to_lowercase();
    state_matches
        && (query.is_empty()
            || project.display_name().to_lowercase().contains(&query)
            || project
                .path
                .to_string_lossy()
                .to_lowercase()
                .contains(&query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn searches_name_and_path_case_insensitively() {
        let entry = ProjectEntry {
            name: "桌面项目".into(),
            path: PathBuf::from("/work/Lure"),
        };
        assert!(matches_project(&entry, " 桌面 ", false, ProjectFilter::All));
        assert!(matches_project(&entry, "lURE", false, ProjectFilter::All));
        assert!(!matches_project(&entry, "other", false, ProjectFilter::All));
    }

    #[test]
    fn filters_only_process_state() {
        let entry = ProjectEntry {
            name: String::new(),
            path: PathBuf::from("/work/demo"),
        };
        assert!(matches_project(
            &entry,
            "demo",
            true,
            ProjectFilter::Running
        ));
        assert!(!matches_project(&entry, "", false, ProjectFilter::Running));
        assert!(matches_project(&entry, "", false, ProjectFilter::Idle));
        assert!(!matches_project(&entry, "", true, ProjectFilter::Idle));
    }
}
