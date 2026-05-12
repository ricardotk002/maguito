use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use crate::git::repo::{self, CommitInfo, FileEntry, SectionKind};

pub struct App {
    pub branch: String,
    pub sections: Vec<Section>,
    pub commits: Vec<CommitInfo>,
    pub commits_collapsed: bool,
    pub cursor: usize,
}

pub struct Section {
    pub kind: SectionKind,
    pub collapsed: bool,
    pub files: Vec<FileNode>,
}

pub struct FileNode {
    pub entry: FileEntry,
    pub collapsed: bool,
}

#[derive(Clone)]
pub enum CursorItem {
    Section(usize),
    File(usize, usize),
    Hunk(usize, usize, usize),
    CommitHeader,
    Commit(usize),
}

impl App {
    pub fn new() -> Result<Self> {
        let status = repo::load()?;
        Ok(Self::from_status(status))
    }

    pub fn from_status(status: repo::RepoStatus) -> Self {
        let sections = status
            .sections
            .into_iter()
            .map(|(kind, files)| Section {
                kind,
                collapsed: false,
                // diffs hidden by default — Tab to expand
                files: files.into_iter().map(|e| FileNode { entry: e, collapsed: true }).collect(),
            })
            .collect();
        Self {
            branch: status.branch,
            sections,
            commits: status.commits,
            commits_collapsed: false,
            cursor: 0,
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        let status = repo::load()?;
        let rebuilt = Self::from_status(status);
        self.branch = rebuilt.branch;
        self.sections = rebuilt.sections;
        self.commits = rebuilt.commits;
        self.cursor = self.cursor.min(self.visible_count().saturating_sub(1));
        Ok(())
    }

    pub fn visible_items(&self) -> Vec<CursorItem> {
        let mut items = Vec::new();
        for (si, section) in self.sections.iter().enumerate() {
            if section.files.is_empty() { continue; }
            items.push(CursorItem::Section(si));
            if !section.collapsed {
                for (fi, file) in section.files.iter().enumerate() {
                    items.push(CursorItem::File(si, fi));
                    if !file.collapsed {
                        for (hi, _) in file.entry.hunks.iter().enumerate() {
                            items.push(CursorItem::Hunk(si, fi, hi));
                        }
                    }
                }
            }
        }
        if !self.commits.is_empty() {
            items.push(CursorItem::CommitHeader);
            if !self.commits_collapsed {
                for (i, _) in self.commits.iter().enumerate() {
                    items.push(CursorItem::Commit(i));
                }
            }
        }
        items
    }

    pub fn visible_count(&self) -> usize {
        self.visible_items().len()
    }

    pub fn current_item(&self) -> Option<CursorItem> {
        self.visible_items().into_iter().nth(self.cursor)
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 { self.cursor -= 1; }
    }

    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.visible_count() {
            self.cursor += 1;
        }
    }

    pub fn toggle_collapse(&mut self) {
        match self.current_item() {
            Some(CursorItem::Section(si)) => {
                self.sections[si].collapsed = !self.sections[si].collapsed;
            }
            Some(CursorItem::File(si, fi)) => {
                self.sections[si].files[fi].collapsed = !self.sections[si].files[fi].collapsed;
            }
            Some(CursorItem::CommitHeader) => {
                self.commits_collapsed = !self.commits_collapsed;
            }
            _ => {}
        }
    }

    pub fn stage_current(&mut self) -> Result<()> {
        match self.current_item() {
            Some(CursorItem::Section(si)) => {
                let stageable = matches!(
                    self.sections[si].kind,
                    SectionKind::Untracked | SectionKind::Unstaged
                );
                if stageable {
                    let paths: Vec<String> = self.sections[si].files.iter()
                        .map(|f| f.entry.path.clone()).collect();
                    for path in paths { repo::stage_file(&path)?; }
                }
            }
            Some(CursorItem::File(si, fi)) => {
                let stageable = matches!(
                    self.sections[si].kind,
                    SectionKind::Untracked | SectionKind::Unstaged
                );
                if stageable {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    repo::stage_file(&path)?;
                }
            }
            Some(CursorItem::Hunk(si, fi, hi)) => {
                if self.sections[si].kind == SectionKind::Unstaged {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                    repo::stage_hunk(&path, &hunk)?;
                }
            }
            _ => {}
        }
        self.refresh()
    }

    pub fn unstage_current(&mut self) -> Result<()> {
        match self.current_item() {
            Some(CursorItem::Section(si)) => {
                if self.sections[si].kind == SectionKind::Staged {
                    let paths: Vec<String> = self.sections[si].files.iter()
                        .map(|f| f.entry.path.clone()).collect();
                    for path in paths { repo::unstage_file(&path)?; }
                }
            }
            Some(CursorItem::File(si, fi)) => {
                if self.sections[si].kind == SectionKind::Staged {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    repo::unstage_file(&path)?;
                }
            }
            Some(CursorItem::Hunk(si, fi, hi)) => {
                if self.sections[si].kind == SectionKind::Staged {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                    repo::unstage_hunk(&path, &hunk)?;
                }
            }
            _ => {}
        }
        self.refresh()
    }
}

pub fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;
    loop {
        terminal.draw(|f| crate::ui::status::render(f, &app))?;
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if crate::keys::handle(&mut app, key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::repo::{ChangeKind, CommitInfo, FileEntry, Hunk, HunkLine, RepoStatus, SectionKind};

    fn make_file(path: &str, hunks: usize) -> FileEntry {
        FileEntry {
            path: path.into(),
            kind: ChangeKind::Modified,
            hunks: (0..hunks)
                .map(|i| Hunk {
                    header: format!("@@ -{i},3 +{i},4 @@"),
                    old_start: i as u32, old_lines: 3,
                    new_start: i as u32, new_lines: 4,
                    lines: vec![
                        HunkLine { origin: ' ', content: "ctx".into() },
                        HunkLine { origin: '+', content: "add".into() },
                    ],
                })
                .collect(),
        }
    }

    fn make_app(sections: Vec<(SectionKind, Vec<FileEntry>)>) -> App {
        App::from_status(RepoStatus {
            branch: "main".into(),
            sections,
            commits: vec![],
        })
    }

    // --- visible_items ---

    #[test]
    fn empty_repo_has_no_items() {
        let app = make_app(vec![]);
        assert!(app.visible_items().is_empty());
    }

    #[test]
    fn section_and_file_are_visible() {
        let app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 0)])]);
        let items = app.visible_items();
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], CursorItem::Section(0)));
        assert!(matches!(items[1], CursorItem::File(0, 0)));
    }

    #[test]
    fn files_start_collapsed_so_hunks_hidden() {
        let app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 3)])]);
        let items = app.visible_items();
        // hunks not visible while collapsed
        assert!(!items.iter().any(|i| matches!(i, CursorItem::Hunk(..))));
    }

    #[test]
    fn expand_file_reveals_hunks() {
        let mut app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 2)])]);
        app.cursor = 1; // on the file
        app.toggle_collapse();
        let items = app.visible_items();
        assert!(matches!(items[2], CursorItem::Hunk(0, 0, 0)));
        assert!(matches!(items[3], CursorItem::Hunk(0, 0, 1)));
    }

    #[test]
    fn collapse_section_hides_all_files() {
        let mut app = make_app(vec![(
            SectionKind::Staged,
            vec![make_file("a.rs", 0), make_file("b.rs", 0)],
        )]);
        app.cursor = 0; // on section header
        app.toggle_collapse();
        let items = app.visible_items();
        assert_eq!(items.len(), 1); // only the section header
    }

    #[test]
    fn commits_visible_when_present() {
        let mut app = make_app(vec![]);
        app.commits = vec![
            CommitInfo { sha: "abc1234".into(), message: "first".into() },
            CommitInfo { sha: "def5678".into(), message: "second".into() },
        ];
        let items = app.visible_items();
        assert!(items.iter().any(|i| matches!(i, CursorItem::CommitHeader)));
        assert_eq!(items.iter().filter(|i| matches!(i, CursorItem::Commit(_))).count(), 2);
    }

    #[test]
    fn collapse_commits_hides_list() {
        let mut app = make_app(vec![]);
        app.commits = vec![CommitInfo { sha: "abc".into(), message: "msg".into() }];
        app.cursor = 0; // CommitHeader
        app.toggle_collapse();
        assert!(!app.visible_items().iter().any(|i| matches!(i, CursorItem::Commit(_))));
    }

    // --- cursor movement ---

    #[test]
    fn move_down_advances_cursor() {
        let mut app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 0)])]);
        app.move_down();
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn cursor_does_not_go_below_last_item() {
        let mut app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 0)])]);
        for _ in 0..10 { app.move_down(); }
        assert_eq!(app.cursor, 1); // last item is File(0,0)
    }

    #[test]
    fn cursor_does_not_go_above_zero() {
        let mut app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 0)])]);
        app.move_up();
        assert_eq!(app.cursor, 0);
    }
}
