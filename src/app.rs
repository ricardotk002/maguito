use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use crate::git::repo::{self, CommitInfo, FileEntry, SectionKind};
use crate::transient::Transient;

pub enum ConfirmKind {
    Discard,
    Trash,
    DiscardHunk,
    DiscardBatch(Vec<CursorItem>),
}

pub struct Confirm {
    pub kind: ConfirmKind,
    pub section: usize,
    pub file: usize,
    pub hunk: Option<usize>,
    pub prompt: String,
}

pub struct App {
    pub branch: String,
    pub sections: Vec<Section>,
    pub commits: Vec<CommitInfo>,
    pub commits_collapsed: bool,
    pub cursor: usize,
    pub visual_anchor: Option<usize>,
    pub transient: Option<Transient>,
    pub confirm: Option<Confirm>,
    pub message: Option<String>,
    pub show_help: bool,
}

pub struct Section {
    pub kind: SectionKind,
    pub collapsed: bool,
    pub files: Vec<FileNode>,
}

pub struct FileNode {
    pub entry: FileEntry,
    pub collapsed: bool,
    pub hunk_collapsed: Vec<bool>,
}

#[derive(Clone)]
pub enum CursorItem {
    Section(usize),
    File(usize, usize),
    Hunk(usize, usize, usize),
    DiffLine(usize, usize, usize, usize), // si, fi, hi, li
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
                files: files.into_iter().map(|e| {
                    let n = e.hunks.len();
                    FileNode { entry: e, collapsed: true, hunk_collapsed: vec![false; n] }
                }).collect(),
            })
            .collect();
        Self {
            branch: status.branch,
            sections,
            commits: status.commits,
            commits_collapsed: false,
            cursor: 0,
            visual_anchor: None,
            transient: None,
            confirm: None,
            message: None,
            show_help: false,
        }
    }

    pub fn refresh(&mut self) -> Result<()> {
        let status = repo::load()?;
        let mut rebuilt = Self::from_status(status);

        // Carry over collapse state so expanded files/hunks stay open after refresh.
        rebuilt.commits_collapsed = self.commits_collapsed;
        for new_sec in &mut rebuilt.sections {
            if let Some(old_sec) = self.sections.iter().find(|s| s.kind == new_sec.kind) {
                new_sec.collapsed = old_sec.collapsed;
                for new_file in &mut new_sec.files {
                    if let Some(old_file) = old_sec.files.iter()
                        .find(|f| f.entry.path == new_file.entry.path)
                    {
                        new_file.collapsed = old_file.collapsed;
                        for (i, c) in new_file.hunk_collapsed.iter_mut().enumerate() {
                            if let Some(&old_c) = old_file.hunk_collapsed.get(i) {
                                *c = old_c;
                            }
                        }
                    }
                }
            }
        }

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
                        for (hi, hunk) in file.entry.hunks.iter().enumerate() {
                            items.push(CursorItem::Hunk(si, fi, hi));
                            if !file.hunk_collapsed[hi] {
                                for (li, _) in hunk.lines.iter().enumerate() {
                                    items.push(CursorItem::DiffLine(si, fi, hi, li));
                                }
                            }
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
            Some(CursorItem::Hunk(si, fi, hi)) => {
                let c = &mut self.sections[si].files[fi].hunk_collapsed[hi];
                *c = !*c;
            }
            Some(CursorItem::CommitHeader) => {
                self.commits_collapsed = !self.commits_collapsed;
            }
            _ => {}
        }
    }

    pub fn discard_current(&mut self) {
        match self.current_item() {
            Some(CursorItem::File(si, fi)) => {
                let section = &self.sections[si];
                let path = section.files[fi].entry.path.clone();
                let (kind, prompt) = match section.kind {
                    SectionKind::Untracked => (
                        ConfirmKind::Trash,
                        format!("Trash \"{}\"? (y or n)", path),
                    ),
                    _ => (
                        ConfirmKind::Discard,
                        format!("Discard unstaged changes in \"{}\"? (y or n)", path),
                    ),
                };
                self.confirm = Some(Confirm { kind, section: si, file: fi, hunk: None, prompt });
            }
            Some(CursorItem::Hunk(si, fi, hi)) => {
                let path = self.sections[si].files[fi].entry.path.clone();
                self.confirm = Some(Confirm {
                    kind: ConfirmKind::DiscardHunk,
                    section: si,
                    file: fi,
                    hunk: Some(hi),
                    prompt: format!("Discard hunk from \"{}\"? (y or n)", path),
                });
            }
            Some(CursorItem::DiffLine(si, fi, hi, li)) => {
                let origin = self.sections[si].files[fi].entry.hunks[hi].lines[li].origin;
                if origin == '+' || origin == '-' {
                    self.confirm = Some(Confirm {
                        kind: ConfirmKind::DiscardBatch(vec![CursorItem::DiffLine(si, fi, hi, li)]),
                        section: si, file: fi, hunk: None,
                        prompt: "Discard this line? (y or n)".into(),
                    });
                }
            }
            _ => {}
        }
    }

    pub fn execute_confirm(&mut self) -> Result<()> {
        if let Some(c) = self.confirm.take() {
            match c.kind {
                ConfirmKind::Discard => {
                    let path = self.sections[c.section].files[c.file].entry.path.clone();
                    let staged = self.sections[c.section].kind == SectionKind::Staged;
                    repo::discard_file(&path, staged)?;
                }
                ConfirmKind::Trash => {
                    let path = self.sections[c.section].files[c.file].entry.path.clone();
                    repo::trash_file(&path)?;
                }
                ConfirmKind::DiscardHunk => {
                    let path = self.sections[c.section].files[c.file].entry.path.clone();
                    let hi = c.hunk.unwrap();
                    let hunk = self.sections[c.section].files[c.file].entry.hunks[hi].clone();
                    let staged = self.sections[c.section].kind == SectionKind::Staged;
                    repo::discard_hunk(&path, &hunk, staged)?;
                }
                ConfirmKind::DiscardBatch(items) => {
                    // Collect DiffLine items grouped by hunk; apply others immediately.
                    let mut hunk_lines: std::collections::HashMap<(usize, usize, usize), Vec<usize>> = std::collections::HashMap::new();
                    for item in &items {
                        match *item {
                            CursorItem::File(si, fi) => {
                                let path = self.sections[si].files[fi].entry.path.clone();
                                match self.sections[si].kind {
                                    SectionKind::Untracked => repo::trash_file(&path)?,
                                    _ => {
                                        let staged = self.sections[si].kind == SectionKind::Staged;
                                        repo::discard_file(&path, staged)?;
                                    }
                                }
                            }
                            CursorItem::Hunk(si, fi, hi) => {
                                let path = self.sections[si].files[fi].entry.path.clone();
                                let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                                let staged = self.sections[si].kind == SectionKind::Staged;
                                repo::discard_hunk(&path, &hunk, staged)?;
                            }
                            CursorItem::DiffLine(si, fi, hi, li) => {
                                hunk_lines.entry((si, fi, hi)).or_default().push(li);
                            }
                            _ => {}
                        }
                    }
                    for ((si, fi, hi), lines) in hunk_lines {
                        let path = self.sections[si].files[fi].entry.path.clone();
                        let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                        let staged = self.sections[si].kind == SectionKind::Staged;
                        repo::discard_lines(&path, &hunk, &lines, staged)?;
                    }
                }
            }
            self.refresh()?;
        }
        Ok(())
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
            Some(CursorItem::DiffLine(si, fi, hi, li)) => {
                if self.sections[si].kind == SectionKind::Unstaged {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                    repo::stage_lines(&path, &hunk, &[li])?;
                }
            }
            _ => {}
        }
        self.refresh()
    }

    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual_anchor.map(|anchor| {
            (anchor.min(self.cursor), anchor.max(self.cursor))
        })
    }

    pub fn stage_visual(&mut self) -> Result<()> {
        let Some((lo, hi)) = self.visual_range() else { return self.stage_current() };
        let items = self.visible_items();
        let mut hunk_lines: std::collections::HashMap<(usize, usize, usize), Vec<usize>> = std::collections::HashMap::new();
        for i in lo..=hi {
            match items.get(i) {
                Some(CursorItem::File(si, fi)) => {
                    let stageable = matches!(self.sections[*si].kind, SectionKind::Untracked | SectionKind::Unstaged);
                    if stageable {
                        repo::stage_file(&self.sections[*si].files[*fi].entry.path)?;
                    }
                }
                Some(CursorItem::Hunk(si, fi, hi_)) => {
                    if self.sections[*si].kind == SectionKind::Unstaged {
                        let path = self.sections[*si].files[*fi].entry.path.clone();
                        let hunk = self.sections[*si].files[*fi].entry.hunks[*hi_].clone();
                        repo::stage_hunk(&path, &hunk)?;
                    }
                }
                Some(CursorItem::DiffLine(si, fi, hi_, li)) => {
                    if self.sections[*si].kind == SectionKind::Unstaged {
                        hunk_lines.entry((*si, *fi, *hi_)).or_default().push(*li);
                    }
                }
                _ => {}
            }
        }
        for ((si, fi, hi_), lines) in hunk_lines {
            let path = self.sections[si].files[fi].entry.path.clone();
            let hunk = self.sections[si].files[fi].entry.hunks[hi_].clone();
            repo::stage_lines(&path, &hunk, &lines)?;
        }
        self.visual_anchor = None;
        self.refresh()
    }

    pub fn unstage_visual(&mut self) -> Result<()> {
        let Some((lo, hi)) = self.visual_range() else { return self.unstage_current() };
        let items = self.visible_items();
        let mut hunk_lines: std::collections::HashMap<(usize, usize, usize), Vec<usize>> = std::collections::HashMap::new();
        for i in lo..=hi {
            match items.get(i) {
                Some(CursorItem::File(si, fi)) => {
                    if self.sections[*si].kind == SectionKind::Staged {
                        repo::unstage_file(&self.sections[*si].files[*fi].entry.path)?;
                    }
                }
                Some(CursorItem::Hunk(si, fi, hi_)) => {
                    if self.sections[*si].kind == SectionKind::Staged {
                        let path = self.sections[*si].files[*fi].entry.path.clone();
                        let hunk = self.sections[*si].files[*fi].entry.hunks[*hi_].clone();
                        repo::unstage_hunk(&path, &hunk)?;
                    }
                }
                Some(CursorItem::DiffLine(si, fi, hi_, li)) => {
                    if self.sections[*si].kind == SectionKind::Staged {
                        hunk_lines.entry((*si, *fi, *hi_)).or_default().push(*li);
                    }
                }
                _ => {}
            }
        }
        for ((si, fi, hi_), lines) in hunk_lines {
            let path = self.sections[si].files[fi].entry.path.clone();
            let hunk = self.sections[si].files[fi].entry.hunks[hi_].clone();
            repo::unstage_lines(&path, &hunk, &lines)?;
        }
        self.visual_anchor = None;
        self.refresh()
    }

    pub fn stash_visual(&mut self) -> Result<()> {
        let Some((lo, hi)) = self.visual_range() else { return Ok(()) };
        let items = self.visible_items();
        let mut paths: Vec<String> = Vec::new();
        for i in lo..=hi {
            if let Some(CursorItem::File(si, fi)) = items.get(i) {
                paths.push(self.sections[*si].files[*fi].entry.path.clone());
            }
        }
        if paths.is_empty() { self.visual_anchor = None; return Ok(()); }
        self.visual_anchor = None;
        repo::stash_push_paths(&paths)?;
        self.refresh()
    }

    pub fn discard_visual(&mut self) {
        let Some((lo, hi)) = self.visual_range() else { self.discard_current(); return };
        let items = self.visible_items();
        let batch: Vec<CursorItem> = (lo..=hi)
            .filter_map(|i| items.get(i).cloned())
            .filter(|item| matches!(item, CursorItem::File(..) | CursorItem::Hunk(..) | CursorItem::DiffLine(..)))
            .collect();
        if batch.is_empty() { self.visual_anchor = None; return; }
        let n = batch.len();
        let prompt = format!("Discard {} item(s)? (y or n)", n);
        self.visual_anchor = None;
        self.confirm = Some(Confirm {
            kind: ConfirmKind::DiscardBatch(batch),
            section: 0, file: 0, hunk: None,
            prompt,
        });
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
            Some(CursorItem::DiffLine(si, fi, hi, li)) => {
                if self.sections[si].kind == SectionKind::Staged {
                    let path = self.sections[si].files[fi].entry.path.clone();
                    let hunk = self.sections[si].files[fi].entry.hunks[hi].clone();
                    repo::unstage_lines(&path, &hunk, &[li])?;
                }
            }
            _ => {}
        }
        self.refresh()
    }
}

pub fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    use crate::keys::KeyAction;

    let mut app = App::new()?;
    loop {
        terminal.draw(|f| {
            crate::ui::status::render(f, &app);
            if app.transient.is_some() {
                crate::ui::transient::render(f, &app);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match crate::keys::handle(&mut app, key)? {
                    KeyAction::Quit => break,

                    KeyAction::CommitCreate => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        let has_staged = app.sections.iter().any(|s| s.kind == SectionKind::Staged)
                            || flags.contains(&"--all")
                            || flags.contains(&"--allow-empty");
                        if !has_staged {
                            app.message = Some("Nothing staged (use -a to stage all)".into());
                        } else {
                            let mut out: Option<String> = None;
                            let r = suspend(terminal, || {
                                if let Some(msg) = open_commit_editor(None)? {
                                    out = Some(repo::commit(&msg, &flags)?);
                                }
                                Ok(())
                            });
                            app.message = match r {
                                Ok(()) => out,
                                Err(e) => Some(format!("{:#}", e)),
                            };
                            app.refresh()?;
                        }
                    }

                    KeyAction::CommitAmend => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        let prefill = repo::head_message().ok();
                        let mut out: Option<String> = None;
                        let r = suspend(terminal, || {
                            if let Some(msg) = open_commit_editor(prefill.as_deref())? {
                                out = Some(repo::amend(&msg, &flags)?);
                            }
                            Ok(())
                        });
                        app.message = match r { Ok(()) => out, Err(e) => Some(format!("{:#}", e)) };
                        app.refresh()?;
                    }

                    KeyAction::CommitReword => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        let prefill = repo::head_message().ok();
                        let mut out: Option<String> = None;
                        let r = suspend(terminal, || {
                            if let Some(msg) = open_commit_editor(prefill.as_deref())? {
                                out = Some(repo::reword(&msg, &flags)?);
                            }
                            Ok(())
                        });
                        app.message = match r { Ok(()) => out, Err(e) => Some(format!("{:#}", e)) };
                        app.refresh()?;
                    }

                    KeyAction::CommitExtend => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        app.message = Some(repo::extend(&flags).unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::ConfirmYes => {
                        if let Err(e) = app.execute_confirm() {
                            app.message = Some(format!("{:#}", e));
                        }
                    }

                    KeyAction::FetchFromPushRemote => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_push_remote() {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok(remote) => {
                                app.message = Some(format!("Fetching from {remote}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::fetch(&remote, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::FetchFromUpstream => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_upstream() {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok((remote, _)) => {
                                app.message = Some(format!("Fetching from {remote}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::fetch(&remote, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::FetchAll => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        app.message = Some("Fetching all remotes…".into());
                        draw_app(terminal, &app)?;
                        app.message = Some(repo::fetch_all(&flags).unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::PushToPushRemote => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_push_remote() {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok(remote) => {
                                app.message = Some(format!("Pushing to {remote}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::push(&remote, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::PushToUpstream => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_upstream() {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok((remote, branch)) => {
                                app.message = Some(format!("Pushing to {remote}/{branch}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::push_to_upstream(&remote, &branch, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::PullFromPushRemote => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_push_remote().and_then(|r| repo::current_branch().map(|b| (r, b))) {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok((remote, branch)) => {
                                app.message = Some(format!("Pulling from {remote}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::pull(&remote, &branch, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::PullFromUpstream => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        match repo::get_upstream() {
                            Err(e) => app.message = Some(format!("{:#}", e)),
                            Ok((remote, branch)) => {
                                app.message = Some(format!("Pulling from {remote}/{branch}…"));
                                draw_app(terminal, &app)?;
                                app.message = Some(repo::pull(&remote, &branch, &flags).unwrap_or_else(|e| format!("{:#}", e)));
                                app.refresh()?;
                            }
                        }
                    }

                    KeyAction::StashBoth => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        app.message = Some(repo::stash_push(&flags).unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashIndex => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        app.message = Some(repo::stash_push_staged(&flags).unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashKeepIndex => {
                        let flags = app.transient.take().map(|t| t.flags_vec()).unwrap_or_default();
                        app.message = Some(repo::stash_push_keep_index(&flags).unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashPop => {
                        app.transient = None;
                        app.message = Some(repo::stash_pop().unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashApply => {
                        app.transient = None;
                        app.message = Some(repo::stash_apply().unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashDrop => {
                        app.transient = None;
                        app.message = Some(repo::stash_drop().unwrap_or_else(|e| format!("{:#}", e)));
                        app.refresh()?;
                    }

                    KeyAction::StashList => {
                        app.transient = None;
                        app.message = Some(repo::stash_list().unwrap_or_else(|e| format!("{:#}", e)));
                    }

                    KeyAction::StashShow => {
                        app.transient = None;
                        app.message = Some(repo::stash_show().unwrap_or_else(|e| format!("{:#}", e)));
                    }

                    KeyAction::Continue => {}
                }
            }
        }
    }
    Ok(())
}

fn leave_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn enter_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}

fn suspend<F: FnOnce() -> Result<()>>(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, f: F) -> Result<()> {
    leave_tui(terminal)?;
    let result = f();
    enter_tui(terminal)?;
    result
}

fn draw_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &App) -> Result<()> {
    terminal.draw(|f| {
        crate::ui::status::render(f, app);
        if app.transient.is_some() { crate::ui::transient::render(f, app); }
    })?;
    Ok(())
}

fn open_commit_editor(prefill: Option<&str>) -> Result<Option<String>> {
    use std::io::Write;

    let tmp = std::env::temp_dir().join("maguito_COMMIT_EDITMSG");
    {
        let mut f = std::fs::File::create(&tmp)?;
        if let Some(msg) = prefill {
            writeln!(f, "{msg}")?;
        } else {
            writeln!(f)?;
        }
        writeln!(f, "# Please enter the commit message for your changes.")?;
        writeln!(f, "# Lines starting with '#' will be ignored.")?;
        writeln!(f, "# An empty message aborts the commit.")?;
    }

    let editor = std::env::var("GIT_EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());

    let mut parts = editor.split_whitespace();
    let bin = parts.next().unwrap_or("vi");
    std::process::Command::new(bin)
        .args(parts)
        .arg(&tmp)
        .status()
        .context("failed to open editor")?;

    let content = std::fs::read_to_string(&tmp)?;
    let message = content
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let message = message.trim().to_string();

    Ok(if message.is_empty() { None } else { Some(message) })
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
        // Section + File + Hunk0 + 2 DiffLines + Hunk1 + 2 DiffLines
        assert!(matches!(items[2], CursorItem::Hunk(0, 0, 0)));
        assert!(matches!(items[3], CursorItem::DiffLine(0, 0, 0, 0)));
        assert!(matches!(items[4], CursorItem::DiffLine(0, 0, 0, 1)));
        assert!(matches!(items[5], CursorItem::Hunk(0, 0, 1)));
    }

    #[test]
    fn collapse_hunk_hides_its_lines_in_renderer() {
        let mut app = make_app(vec![(SectionKind::Staged, vec![make_file("a.rs", 2)])]);
        // expand the file
        app.cursor = 1;
        app.toggle_collapse();
        // both hunks are expanded by default
        assert!(!app.sections[0].files[0].hunk_collapsed[0]);
        assert!(!app.sections[0].files[0].hunk_collapsed[1]);
        // collapse hunk 0
        app.cursor = 2; // on Hunk(0,0,0)
        app.toggle_collapse();
        assert!(app.sections[0].files[0].hunk_collapsed[0]);
        assert!(!app.sections[0].files[0].hunk_collapsed[1]);
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
