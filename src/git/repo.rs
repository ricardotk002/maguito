use anyhow::{Context, Result};
use git2::{Delta, Repository, Status, StatusOptions};
use std::path::Path;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub enum SectionKind {
    Untracked,
    Staged,
    Unstaged,
}

#[derive(Debug, Clone)]
pub enum ChangeKind {
    Untracked,
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl ChangeKind {
    pub fn label(&self) -> &'static str {
        match self {
            ChangeKind::Untracked => "",
            ChangeKind::Added     => "new file",
            ChangeKind::Modified  => "modified",
            ChangeKind::Deleted   => "deleted",
            ChangeKind::Renamed   => "renamed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub kind: ChangeKind,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<HunkLine>,
}

#[derive(Debug, Clone)]
pub struct HunkLine {
    pub origin: char,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
}

pub struct RepoStatus {
    pub branch: String,
    pub sections: Vec<(SectionKind, Vec<FileEntry>)>,
    pub commits: Vec<CommitInfo>,
}

#[derive(Debug)]
enum DiffEvent {
    File { path: String, kind: ChangeKind },
    Hunk { header: String, old_start: u32, old_lines: u32, new_start: u32, new_lines: u32 },
    Line { origin: char, content: String },
}

pub fn load() -> Result<RepoStatus> {
    load_from(Path::new("."))
}

pub fn load_from(path: &Path) -> Result<RepoStatus> {
    let repo = Repository::discover(path).context("not a git repository")?;
    let mut result = RepoStatus {
        branch: String::new(),
        sections: Vec::new(),
        commits: Vec::new(),
    };

    result.branch = match repo.head() {
        Ok(head) => head.shorthand().unwrap_or("HEAD").to_string(),
        Err(_) => "(no commits yet)".to_string(),
    };

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    let untracked: Vec<FileEntry> = statuses
        .iter()
        .filter(|e| e.status().contains(Status::WT_NEW))
        .map(|e| FileEntry {
            path: e.path().unwrap_or("").to_string(),
            kind: ChangeKind::Untracked,
            hunks: vec![],
        })
        .collect();
    if !untracked.is_empty() {
        result.sections.push((SectionKind::Untracked, untracked));
    }

    // Staged before unstaged
    let staged = collect_diff_files(&repo, true)?;
    if !staged.is_empty() {
        result.sections.push((SectionKind::Staged, staged));
    }

    let unstaged = collect_diff_files(&repo, false)?;
    if !unstaged.is_empty() {
        result.sections.push((SectionKind::Unstaged, unstaged));
    }

    result.commits = load_recent_commits(&repo, 10).unwrap_or_default();

    Ok(result)
}

fn collect_diff_files(repo: &Repository, staged: bool) -> Result<Vec<FileEntry>> {
    let diff = if staged {
        match repo.head() {
            Ok(head) => {
                let tree = head.peel_to_tree()?;
                repo.diff_tree_to_index(Some(&tree), None, None)?
            }
            Err(_) => repo.diff_tree_to_index(None, None, None)?,
        }
    } else {
        repo.diff_index_to_workdir(None, None)?
    };

    let events: Rc<RefCell<Vec<DiffEvent>>> = Rc::new(RefCell::new(Vec::new()));

    {
        let ev = Rc::clone(&events);
        let mut file_cb = move |delta: git2::DiffDelta<'_>, _: f32| -> bool {
            let path = delta.new_file().path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .to_string();
            let kind = match delta.status() {
                Delta::Added    => ChangeKind::Added,
                Delta::Deleted  => ChangeKind::Deleted,
                Delta::Renamed  => ChangeKind::Renamed,
                _               => ChangeKind::Modified,
            };
            ev.borrow_mut().push(DiffEvent::File { path, kind });
            true
        };

        let ev = Rc::clone(&events);
        let mut hunk_cb = move |_: git2::DiffDelta<'_>, hunk: git2::DiffHunk<'_>| -> bool {
            let header = std::str::from_utf8(hunk.header()).unwrap_or("").trim_end().to_string();
            ev.borrow_mut().push(DiffEvent::Hunk {
                header,
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
            });
            true
        };

        let ev = Rc::clone(&events);
        let mut line_cb = move |_: git2::DiffDelta<'_>, _: Option<git2::DiffHunk<'_>>, line: git2::DiffLine<'_>| -> bool {
            let origin = line.origin();
            if matches!(origin, '+' | '-' | ' ') {
                let content = std::str::from_utf8(line.content()).unwrap_or("").trim_end().to_string();
                ev.borrow_mut().push(DiffEvent::Line { origin, content });
            }
            true
        };

        diff.foreach(&mut file_cb, None, Some(&mut hunk_cb), Some(&mut line_cb))?;
    }

    let mut files: Vec<FileEntry> = Vec::new();
    for event in Rc::try_unwrap(events).unwrap().into_inner() {
        match event {
            DiffEvent::File { path, kind } => files.push(FileEntry { path, kind, hunks: vec![] }),
            DiffEvent::Hunk { header, old_start, old_lines, new_start, new_lines } => {
                if let Some(f) = files.last_mut() {
                    f.hunks.push(Hunk { header, old_start, old_lines, new_start, new_lines, lines: vec![] });
                }
            }
            DiffEvent::Line { origin, content } => {
                if let Some(f) = files.last_mut() {
                    if let Some(h) = f.hunks.last_mut() {
                        h.lines.push(HunkLine { origin, content });
                    }
                }
            }
        }
    }

    Ok(files)
}

fn load_recent_commits(repo: &Repository, count: usize) -> Result<Vec<CommitInfo>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    let mut commits = Vec::new();
    for oid in revwalk.take(count) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        commits.push(CommitInfo {
            sha: format!("{:.7}", oid),
            message: commit.summary().unwrap_or("").to_string(),
        });
    }
    Ok(commits)
}

pub fn head_message() -> Result<String> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?.peel_to_commit()?;
    Ok(head.message().unwrap_or("").trim().to_string())
}

pub fn commit(message: &str, flags: &[&str]) -> Result<()> {
    commit_from(Path::new("."), message, flags)
}

pub fn commit_from(repo_path: &Path, message: &str, flags: &[&str]) -> Result<()> {
    let repo = Repository::discover(repo_path)?;
    let sig = repo.signature().context("git user.name/email not configured")?;
    let mut index = repo.index()?;
    if flags.contains(&"--all") {
        stage_all_modified(&repo, &mut index)?;
    }
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    let msg = apply_message_flags(message, &sig, flags);
    repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parents)?;
    Ok(())
}

pub fn amend(message: &str, flags: &[&str]) -> Result<()> {
    let repo = Repository::discover(".")?;
    let sig = repo.signature()?;
    let head = repo.head()?.peel_to_commit()?;
    let mut index = repo.index()?;
    if flags.contains(&"--all") {
        stage_all_modified(&repo, &mut index)?;
    }
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let msg = apply_message_flags(message, &sig, flags);
    head.amend(Some("HEAD"), Some(&sig), Some(&sig), None, Some(&msg), Some(&tree))?;
    Ok(())
}

pub fn reword(message: &str, flags: &[&str]) -> Result<()> {
    let repo = Repository::discover(".")?;
    let sig = repo.signature()?;
    let head = repo.head()?.peel_to_commit()?;
    let msg = apply_message_flags(message, &sig, flags);
    head.amend(Some("HEAD"), Some(&sig), Some(&sig), None, Some(&msg), None)?;
    Ok(())
}

pub fn extend(flags: &[&str]) -> Result<()> {
    let repo = Repository::discover(".")?;
    let sig = repo.signature()?;
    let head = repo.head()?.peel_to_commit()?;
    let existing_msg = head.message().unwrap_or("").to_string();
    let mut index = repo.index()?;
    if flags.contains(&"--all") {
        stage_all_modified(&repo, &mut index)?;
    }
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    head.amend(Some("HEAD"), Some(&sig), Some(&sig), None, Some(&existing_msg), Some(&tree))?;
    Ok(())
}

fn apply_message_flags(message: &str, sig: &git2::Signature, flags: &[&str]) -> String {
    if flags.contains(&"--signoff") {
        format!(
            "{}\n\nSigned-off-by: {} <{}>",
            message.trim(),
            sig.name().unwrap_or(""),
            sig.email().unwrap_or(""),
        )
    } else {
        message.to_string()
    }
}

fn stage_all_modified(repo: &Repository, index: &mut git2::Index) -> Result<()> {
    let statuses = repo.statuses(None)?;
    for s in statuses.iter() {
        let st = s.status();
        if st.intersects(Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_RENAMED | Status::WT_TYPECHANGE) {
            if let Some(path) = s.path() {
                if st.contains(Status::WT_DELETED) {
                    let _ = index.remove_path(Path::new(path));
                } else {
                    let _ = index.add_path(Path::new(path));
                }
            }
        }
    }
    index.write()?;
    Ok(())
}

pub fn stage_file(path: &str) -> Result<()> {
    let repo = Repository::discover(".")?;
    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    Ok(())
}

pub fn unstage_file(path: &str) -> Result<()> {
    let repo = Repository::discover(".")?;
    match repo.head() {
        Ok(head) => {
            let commit = head.peel_to_commit()?;
            repo.reset_default(Some(commit.as_object()), std::iter::once(path))?;
        }
        Err(_) => {
            let mut index = repo.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}

pub fn stage_hunk(file_path: &str, hunk: &Hunk) -> Result<()> {
    run_git_apply(&build_patch(file_path, hunk), false)
}

pub fn unstage_hunk(file_path: &str, hunk: &Hunk) -> Result<()> {
    run_git_apply(&build_patch(file_path, hunk), true)
}

fn build_patch(file_path: &str, hunk: &Hunk) -> String {
    let mut s = String::new();
    s.push_str(&format!("diff --git a/{file_path} b/{file_path}\n"));
    s.push_str(&format!("--- a/{file_path}\n"));
    s.push_str(&format!("+++ b/{file_path}\n"));
    s.push_str(&hunk.header);
    s.push('\n');
    for line in &hunk.lines {
        s.push(line.origin);
        s.push_str(&line.content);
        s.push('\n');
    }
    s
}

fn run_git_apply(patch: &str, reverse: bool) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut args = vec!["apply", "--cached"];
    if reverse { args.push("--reverse"); }
    args.push("-");

    let mut child = Command::new("git")
        .args(&args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run git apply")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes())?;
    }

    let out = child.wait_with_output()?;
    if !out.status.success() {
        anyhow::bail!("git apply failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}
