use std::{fs, io::ErrorKind, path::Path};

use crate::routes::SidebarConfig;

pub fn load_sidebar_config_from_root(root: &Path) -> anyhow::Result<SidebarConfig> {
    let path = root.join(".notesmith").join("sidebar.yaml");
    match fs::read_to_string(&path) {
        Ok(raw) => Ok(serde_yaml::from_str::<SidebarConfig>(&raw)?),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(SidebarConfig { views: vec![] }),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{FolderSort, ItemSource, RecentlyViewedMode, SidebarSection, SortDir};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn load_sidebar_config_returns_empty_when_file_missing() {
        let temp_dir = TempDir::new().unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        assert_eq!(config, SidebarConfig { views: vec![] });
    }

    #[test]
    fn load_sidebar_config_parses_full_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: customers
    name: Customers
    icon: "🏢"
    badge_query: "SELECT COUNT(*) FROM v_customers"
    sections:
      - type: recently-viewed
        label: Recent
        mode: edited
        limit: 5
      - type: custom-folders
        label: Key Folders
        folders:
          - Customers
          - Projects
      - type: custom-items
        label: Dashboards
        items:
          - name: Pipeline
            icon: "📊"
            source:
              query: "SELECT path, title FROM v_notes WHERE type = 'dashboard'"
              title_column: title
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        assert_eq!(config.views.len(), 1);
        let view = &config.views[0];
        assert_eq!(view.id, "customers");
        assert_eq!(view.name, "Customers");
        assert_eq!(view.icon, "🏢");
        assert_eq!(
            view.badge_query,
            Some("SELECT COUNT(*) FROM v_customers".to_string())
        );
        assert_eq!(view.sections.len(), 3);

        match &view.sections[0] {
            SidebarSection::RecentlyViewed { label, mode, limit } => {
                assert_eq!(label, "Recent");
                assert_eq!(*mode, RecentlyViewedMode::Edited);
                assert_eq!(*limit, 5);
            }
            other => panic!("expected RecentlyViewed, got {other:?}"),
        }

        match &view.sections[1] {
            SidebarSection::CustomFolders { label, folders } => {
                assert_eq!(label, "Key Folders");
                assert_eq!(folders, &["Customers", "Projects"]);
            }
            other => panic!("expected CustomFolders, got {other:?}"),
        }

        match &view.sections[2] {
            SidebarSection::CustomItems { label, items } => {
                assert_eq!(label, "Dashboards");
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].name, "Pipeline");
            }
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_handles_folder_source() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: work
    name: Work
    icon: "💼"
    sections:
      - type: custom-items
        label: Active Projects
        items:
          - name: Projects
            icon: "📁"
            source:
              folder: Projects/Active
              recursive: true
              sort: name
              sort_dir: asc
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        let view = &config.views[0];
        match &view.sections[0] {
            SidebarSection::CustomItems { items, .. } => match &items[0].source {
                ItemSource::Folder(fs) => {
                    assert_eq!(fs.folder, "Projects/Active");
                    assert!(fs.recursive);
                    assert_eq!(fs.sort, FolderSort::Name);
                    assert_eq!(fs.sort_dir, SortDir::Asc);
                }
                other => panic!("expected FolderSource, got {other:?}"),
            },
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_handles_query_source() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            r#"views:
  - id: metrics
    name: Metrics
    icon: "📈"
    sections:
      - type: custom-items
        label: Reports
        items:
          - name: Weekly
            icon: "📅"
            source:
              query: "SELECT path, title, status FROM v_reports"
              title_column: title
              subtitle_column: status
              badge_columns:
                - status
"#,
        )
        .unwrap();

        let config = load_sidebar_config_from_root(temp_dir.path()).unwrap();

        let view = &config.views[0];
        match &view.sections[0] {
            SidebarSection::CustomItems { items, .. } => match &items[0].source {
                ItemSource::Query(qs) => {
                    assert_eq!(qs.query, "SELECT path, title, status FROM v_reports");
                    assert_eq!(qs.title_column, Some("title".to_string()));
                    assert_eq!(qs.subtitle_column, Some("status".to_string()));
                    assert_eq!(qs.badge_columns, vec!["status"]);
                }
                other => panic!("expected QuerySource, got {other:?}"),
            },
            other => panic!("expected CustomItems, got {other:?}"),
        }
    }

    #[test]
    fn load_sidebar_config_returns_error_on_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".notesmith");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("sidebar.yaml"),
            "views:\n  - id: [invalid yaml\n",
        )
        .unwrap();

        let result = load_sidebar_config_from_root(temp_dir.path());
        assert!(result.is_err());
    }
}
