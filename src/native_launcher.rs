//! State and deterministic grouping for Quark's lightweight native launcher.
//! No GTK, AppKit, WinUI, or egui dependency: every frontend uses the same
//! recent-project ordering, pinning, filtering and safe workspace discovery.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_RECENTS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgeGroup {
    Pinned,
    Today,
    ThisWeek,
    ThisMonth,
    Older,
}

impl AgeGroup {
    pub const ALL: [Self; 5] = [
        Self::Pinned,
        Self::Today,
        Self::ThisWeek,
        Self::ThisMonth,
        Self::Older,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Pinned => "Pinned",
            Self::Today => "Today",
            Self::ThisWeek => "This week",
            Self::ThisMonth => "This month",
            Self::Older => "Older",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recent {
    pub path: PathBuf,
    pub opened_at: u64,
    #[serde(default)]
    pub pinned: bool,
}

impl Recent {
    pub fn group(&self, now: u64) -> AgeGroup {
        if self.pinned {
            return AgeGroup::Pinned;
        }
        if self.opened_at / 86_400 == now / 86_400 {
            AgeGroup::Today
        } else {
            let elapsed = now.saturating_sub(self.opened_at);
            if elapsed < 7 * 86_400 {
                AgeGroup::ThisWeek
            } else if elapsed < 31 * 86_400 {
                AgeGroup::ThisMonth
            } else {
                AgeGroup::Older
            }
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub recent: Vec<Recent>,
    pub hidden: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub title: String,
    pub path: Option<PathBuf>,
}

impl History {
    pub fn open(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(path, &bytes).map_err(|e| e.to_string())
    }

    pub fn remember(&mut self, path: PathBuf, now: u64) {
        let path = path.canonicalize().unwrap_or(path);
        self.hidden.retain(|item| item != &path);
        let pinned = self
            .recent
            .iter()
            .find(|entry| entry.path == path)
            .is_some_and(|entry| entry.pinned);
        self.recent.retain(|entry| entry.path != path);
        self.recent.insert(
            0,
            Recent {
                path,
                opened_at: now,
                pinned,
            },
        );
        self.recent.truncate(MAX_RECENTS);
    }

    pub fn migrate_paths(&mut self, paths: &[PathBuf], now: u64) {
        // Legacy paths are already most-recent first. Preserve that order
        // without moving old items above existing pinned/native history.
        for path in paths {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !self.recent.iter().any(|entry| entry.path == canonical)
                && !self.hidden.contains(&canonical)
            {
                self.recent.push(Recent {
                    path: canonical,
                    opened_at: now,
                    pinned: false,
                });
            }
        }
        self.recent.truncate(MAX_RECENTS);
    }

    pub fn toggle_pin(&mut self, path: &Path) -> bool {
        if let Some(entry) = self.recent.iter_mut().find(|entry| entry.path == path) {
            entry.pinned = !entry.pinned;
            return entry.pinned;
        }
        false
    }

    pub fn remove(&mut self, path: &Path) {
        self.recent.retain(|entry| entry.path != path);
        if !self.hidden.iter().any(|item| item == path) {
            self.hidden.push(path.to_path_buf());
            self.hidden.truncate(MAX_RECENTS);
        }
    }

    pub fn rows(&self, query: &str, now: u64) -> Vec<Row> {
        self.rows_collapsed(query, now, &[])
    }

    /// Visible launcher rows with Visual Studio-style collapsible age groups.
    /// Search always expands matching results, even if their group is folded.
    pub fn rows_collapsed(&self, query: &str, now: u64, collapsed: &[AgeGroup]) -> Vec<Row> {
        let query = query.trim().to_lowercase();
        let mut result = Vec::new();
        for group in AgeGroup::ALL {
            let mut entries: Vec<_> = self
                .recent
                .iter()
                .filter(|entry| {
                    entry.group(now) == group
                        && (query.is_empty()
                            || entry.path.to_string_lossy().to_lowercase().contains(&query))
                })
                .collect();
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.opened_at));
            if entries.is_empty() {
                continue;
            }
            let folded = query.is_empty() && collapsed.contains(&group);
            result.push(Row {
                title: format!("{} {}", if folded { "▸" } else { "▾" }, group.label()),
                path: None,
            });
            if folded {
                continue;
            }
            for entry in entries {
                let filename = entry.path.file_stem().unwrap_or_default().to_string_lossy();
                let folder = entry.path.parent().unwrap_or_else(|| Path::new("."));
                result.push(Row {
                    title: format!(
                        "    {}{}\n        {}",
                        if entry.pinned { "★ " } else { "" },
                        filename,
                        folder.display()
                    ),
                    path: Some(entry.path.clone()),
                });
            }
        }
        result
    }
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn clone_request(url: &str, parent: &Path, name: &str) -> Result<PathBuf, String> {
    let url = url.trim();
    let github_https = url.starts_with("https://github.com/");
    let github_ssh = url.starts_with("git@github.com:");
    if !(github_https || github_ssh)
        || url.chars().any(char::is_whitespace)
        || url.contains("..")
        || url.starts_with('-')
    {
        return Err("Enter a GitHub HTTPS or SSH repository URL".into());
    }
    if !crate::startup::valid_name(name) || !parent.is_dir() {
        return Err("Choose a valid folder and repository name".into());
    }
    let destination = parent.join(name);
    if destination.exists() {
        return Err("The destination folder already exists".into());
    }
    Ok(destination)
}

pub fn first_project(folder: &Path) -> Option<PathBuf> {
    let mut projects = fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("hyproj"))
        .collect::<Vec<_>>();
    projects.sort();
    projects.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_groups_can_collapse_without_hiding_search_results() {
        let mut history = History::default();
        history.recent = vec![Recent {
            path: PathBuf::from("/tmp/QuarkDemo.hyproj"),
            opened_at: 200_000,
            pinned: true,
        }];
        let collapsed = history.rows_collapsed("", 200_100, &[AgeGroup::Pinned]);
        assert_eq!(collapsed.len(), 1);
        assert!(collapsed[0].title.contains("Pinned"));
        assert!(collapsed[0].path.is_none());
        let searched = history.rows_collapsed("quark", 200_100, &[AgeGroup::Pinned]);
        assert_eq!(searched.len(), 2);
        assert!(searched[1].path.is_some());
    }

    #[test]
    fn expanded_age_groups_have_headers_and_selectable_entries() {
        let mut history = History::default();
        history.recent = vec![Recent {
            path: PathBuf::from("/tmp/Recent.hyproj"),
            opened_at: 200_000,
            pinned: false,
        }];
        let expanded = history.rows_collapsed("", 200_100, &[]);
        assert!(expanded[0].title.contains("Today"));
        assert_eq!(
            expanded[1].path.as_ref().unwrap(),
            &PathBuf::from("/tmp/Recent.hyproj")
        );
    }

    #[test]
    fn recent_projects_group_by_pin_today_week_month_older() {
        let now = 100 * 86_400 + 40_000;
        let mut history = History::default();
        for (name, opened) in [
            ("now", now),
            ("week", now - 4 * 86_400),
            ("month", now - 14 * 86_400),
            ("older", now - 40 * 86_400),
        ] {
            history.remember(PathBuf::from(format!("/fake/{name}.hyproj")), opened);
        }
        assert_eq!(
            history
                .recent
                .iter()
                .find(|x| x.path.ends_with("week.hyproj"))
                .unwrap()
                .group(now),
            AgeGroup::ThisWeek
        );
        let pinned = PathBuf::from("/fake/older.hyproj");
        history.toggle_pin(&pinned);
        assert_eq!(
            history
                .recent
                .iter()
                .find(|x| x.path == pinned)
                .unwrap()
                .group(now),
            AgeGroup::Pinned
        );
        let headings: Vec<_> = history
            .rows("", now)
            .into_iter()
            .filter(|row| row.path.is_none())
            .map(|row| row.title)
            .collect();
        assert_eq!(
            headings,
            ["▾ Pinned", "▾ Today", "▾ This week", "▾ This month"]
        );
    }

    #[test]
    fn recent_search_is_case_insensitive_and_remove_is_non_destructive() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("App.hyproj");
        fs::write(&manifest, "keep me").unwrap();
        let mut history = History::default();
        history.remember(manifest.clone(), 500_000);
        assert_eq!(
            history
                .rows("APP", 500_000)
                .iter()
                .filter(|r| r.path.is_some())
                .count(),
            1
        );
        history.remove(&manifest.canonicalize().unwrap());
        assert!(history.rows("", 500_000).is_empty());
        assert_eq!(fs::read_to_string(manifest).unwrap(), "keep me");
        history.migrate_paths(&[dir.path().join("App.hyproj")], 500_001);
        assert!(history.recent.is_empty());
    }

    #[test]
    fn pin_and_recent_history_survive_persistence_and_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("launcher.json");
        let project = dir.path().join("demo.hyproj");
        fs::write(&project, "format=2").unwrap();
        let mut history = History::default();
        history.remember(project.clone(), 100);
        history.toggle_pin(&project.canonicalize().unwrap());
        history.remember(project.clone(), 500);
        history.save(&file).unwrap();
        let restored = History::open(&file).unwrap();
        assert_eq!(restored.recent.len(), 1);
        assert!(restored.recent[0].pinned);
        assert_eq!(restored.recent[0].opened_at, 500);
    }

    #[test]
    fn migration_keeps_existing_order_and_deduplicates_paths() {
        let mut history = History::default();
        history.migrate_paths(
            &[
                PathBuf::from("/missing/new.hyproj"),
                PathBuf::from("/missing/old.hyproj"),
                PathBuf::from("/missing/new.hyproj"),
            ],
            20,
        );
        assert_eq!(history.recent.len(), 2);
        assert!(history.recent[0].path.ends_with("new.hyproj"));
    }

    #[test]
    fn cloning_rejects_invalid_urls_paths_and_existing_directories() {
        let dir = tempfile::tempdir().unwrap();
        assert!(clone_request(
            "https://github.com/example/project.git",
            dir.path(),
            "Project"
        )
        .is_ok());
        assert!(clone_request("git@github.com:example/project.git", dir.path(), "Project").is_ok());
        assert!(clone_request("https://evil.example/hey.git", dir.path(), "Project").is_err());
        assert!(clone_request("-config", dir.path(), "Project").is_err());
        assert!(clone_request("https://github.com/a/b.git", dir.path(), "../escape").is_err());
        fs::create_dir(dir.path().join("Project")).unwrap();
        assert!(clone_request(
            "https://github.com/example/project.git",
            dir.path(),
            "Project"
        )
        .is_err());
    }

    #[test]
    fn discovers_local_project_manifest_without_opening_or_modifying_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.hyproj"), "format=2").unwrap();
        fs::write(dir.path().join("a.hyproj"), "format=2").unwrap();
        assert_eq!(
            first_project(dir.path()).unwrap().file_name().unwrap(),
            "a.hyproj"
        );
        assert!(first_project(&dir.path().join("absent")).is_none());
    }
}
