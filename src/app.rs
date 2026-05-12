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

    fn from_status(status: repo::RepoStatus) -> Self {
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
