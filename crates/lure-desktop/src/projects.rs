use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEntry {
    #[serde(default)]
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectList {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}

impl ProjectEntry {
    #[must_use]
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .map_or_else(|| self.path.display().to_string(), ToOwned::to_owned)
    }
}

#[must_use]
pub fn projects_file_from_home(home: &Path) -> PathBuf {
    home.join(".pi").join("projects.yaml")
}

/// 读取并规范化项目列表。
///
/// # Errors
///
/// 文件无法读取或 YAML 无法解析时返回错误。
pub fn load_projects(path: &Path) -> Result<ProjectList, String> {
    if !path.exists() {
        return Ok(ProjectList::default());
    }
    let source = fs::read_to_string(path)
        .map_err(|error| format!("无法读取项目列表 {}：{error}", path.display()))?;
    let parsed: ProjectList = serde_yaml::from_str(&source)
        .map_err(|error| format!("项目列表格式无效 {}：{error}", path.display()))?;
    let mut seen = HashSet::new();
    let mut projects = Vec::new();
    for mut project in parsed.projects {
        let Ok(canonical) = project.path.canonicalize() else {
            continue;
        };
        if !canonical.is_dir() || !seen.insert(canonical.clone()) {
            continue;
        }
        project.path = canonical;
        projects.push(project);
    }
    Ok(ProjectList { projects })
}

/// 在覆盖前备份无法解析的项目列表。
///
/// # Errors
///
/// 系统时间不可用或文件无法复制时返回错误。
pub fn backup_invalid_projects(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() || load_projects(path).is_ok() {
        return Ok(None);
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时间无效：{error}"))?
        .as_secs();
    let backup = path.with_file_name(format!("projects.yaml.bak-{timestamp}"));
    fs::copy(path, &backup)
        .map_err(|error| format!("无法备份损坏的项目列表 {}：{error}", path.display()))?;
    Ok(Some(backup))
}

/// 使用临时文件与重命名原子保存项目列表。
///
/// # Errors
///
/// 父目录不存在、序列化失败或文件写入失败时返回错误。
pub fn save_projects(path: &Path, projects: &ProjectList) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("项目列表路径无父目录：{}", path.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "未检测到 Pi 配置目录 {}，请先完成 Pi 安装与初始化",
            parent.display()
        ));
    }
    let yaml =
        serde_yaml::to_string(projects).map_err(|error| format!("无法序列化项目列表：{error}"))?;
    let temporary = path.with_extension("yaml.tmp");
    let mut file = File::create(&temporary)
        .map_err(|error| format!("无法创建临时项目列表 {}：{error}", temporary.display()))?;
    file.write_all(yaml.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("无法写入项目列表 {}：{error}", temporary.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("无法保存项目列表 {}：{error}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lure-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn yaml_round_trip_preserves_empty_names() {
        let list = ProjectList {
            projects: vec![ProjectEntry {
                name: String::new(),
                path: PathBuf::from("/tmp/example"),
            }],
        };
        let yaml = serde_yaml::to_string(&list).unwrap();
        assert_eq!(serde_yaml::from_str::<ProjectList>(&yaml).unwrap(), list);
        assert!(yaml.contains("name: ''"));
    }

    #[test]
    fn display_name_falls_back_to_directory_name() {
        let entry = ProjectEntry {
            name: String::new(),
            path: PathBuf::from("/work/示例项目"),
        };
        assert_eq!(entry.display_name(), "示例项目");
    }

    #[test]
    fn load_filters_missing_and_duplicate_directories() {
        let root = temp_dir("load");
        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let yaml = format!(
            "projects:\n  - name: first\n    path: {}\n  - name: duplicate\n    path: {}\n  - name: missing\n    path: {}/missing\n",
            project.display(),
            project.display(),
            root.display()
        );
        let config = root.join("projects.yaml");
        fs::write(&config, yaml).unwrap();
        let loaded = load_projects(&config).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.projects[0].name, "first");
    }

    #[test]
    fn corrupt_file_is_backed_up_before_replacement() {
        let root = temp_dir("backup");
        let config = root.join("projects.yaml");
        fs::write(&config, "projects: [not valid").unwrap();
        let backup = backup_invalid_projects(&config).unwrap().unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "projects: [not valid");
    }

    #[test]
    fn save_is_atomic_and_does_not_create_parent() {
        let root = temp_dir("save");
        let missing = root.join("missing/projects.yaml");
        assert!(save_projects(&missing, &ProjectList::default()).is_err());

        let project = root.join("project");
        fs::create_dir(&project).unwrap();
        let config = root.join("projects.yaml");
        let list = ProjectList {
            projects: vec![ProjectEntry {
                name: "项目".into(),
                path: project.canonicalize().unwrap(),
            }],
        };
        save_projects(&config, &list).unwrap();
        assert_eq!(load_projects(&config).unwrap(), list);
        assert!(!root.join("projects.yaml.tmp").exists());
    }

    #[test]
    fn project_file_uses_home_pi_directory() {
        assert_eq!(
            projects_file_from_home(Path::new("/Users/test")),
            PathBuf::from("/Users/test/.pi/projects.yaml")
        );
    }
}
