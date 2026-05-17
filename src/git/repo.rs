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

pub fn check() -> Result<()> {
    Repository::discover(".").context("not a git repository")?;
    Ok(())
}

fn workdir() -> Result<std::path::PathBuf> {
    let repo = Repository::discover(".")?;
    repo.workdir()
        .map(|p| p.to_path_buf())
        .context("bare repositories are not supported")
}

pub fn current_branch() -> Result<String> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;
    Ok(head.shorthand().unwrap_or("HEAD").to_string())
}

pub fn get_push_remote() -> Result<String> {
    let repo = Repository::discover(".")?;
    let branch = repo.head()?.shorthand().unwrap_or("HEAD").to_string();
    let config = repo.config()?;
    if let Ok(r) = config.get_string(&format!("branch.{branch}.pushRemote")) {
        return Ok(r);
    }
    if let Ok(r) = config.get_string("remote.pushDefault") {
        return Ok(r);
    }
    let remotes = repo.remotes()?;
    if let Some(Some(r)) = remotes.iter().next() {
        return Ok(r.to_string());
    }
    anyhow::bail!("no push remote configured")
}

pub fn get_upstream() -> Result<(String, String)> {
    let repo = Repository::discover(".")?;
    let branch = repo.head()?.shorthand().unwrap_or("HEAD").to_string();
    let config = repo.config()?;
    let remote = config.get_string(&format!("branch.{branch}.remote"))
        .context("no upstream remote configured (set with git branch --set-upstream-to)")?;
    let merge = config.get_string(&format!("branch.{branch}.merge"))
        .context("no upstream branch configured")?;
    let upstream_branch = merge.strip_prefix("refs/heads/").unwrap_or(&merge).to_string();
    Ok((remote, upstream_branch))
}

pub fn stash_push_paths(paths: &[String]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["stash", "push", "--include-untracked", "--"])
        .args(paths)
        .output()
        .context("failed to run git stash push")?;
    parse_output(&out)
}

pub fn stash_push(flags: &[&str]) -> Result<String> {
    git_output(&["stash", "push"], flags, &[])
}

pub fn stash_push_staged(flags: &[&str]) -> Result<String> {
    git_output(&["stash", "push", "--staged"], flags, &[])
}

pub fn stash_push_keep_index(flags: &[&str]) -> Result<String> {
    git_output(&["stash", "push", "--keep-index"], flags, &[])
}

pub fn stash_pop() -> Result<String> {
    git_output(&["stash", "pop"], &[], &[])
}

pub fn stash_apply() -> Result<String> {
    git_output(&["stash", "apply"], &[], &[])
}

pub fn stash_drop() -> Result<String> {
    git_output(&["stash", "drop"], &[], &[])
}

pub fn stash_list() -> Result<String> {
    let out = std::process::Command::new("git")
        .args(["stash", "list"])
        .output()
        .context("failed to run git stash list")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("{}", err.lines().next().unwrap_or("git stash list failed"));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let entries: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    match entries.len() {
        0 => Ok("No stashes".into()),
        1 => Ok(entries[0].to_string()),
        n => Ok(format!("{n} stashes, latest: {}", entries[0])),
    }
}

pub fn stash_show() -> Result<String> {
    git_output(&["stash", "show"], &[], &[])
}

pub fn fetch(remote: &str, flags: &[&str]) -> Result<String> {
    git_output(&["fetch"], flags, &[remote])
}

pub fn fetch_all(flags: &[&str]) -> Result<String> {
    git_output(&["fetch", "--all"], flags, &[])
}

pub fn push(remote: &str, flags: &[&str]) -> Result<String> {
    git_output(&["push"], flags, &[remote, "HEAD"])
}

pub fn push_to_upstream(remote: &str, upstream_branch: &str, flags: &[&str]) -> Result<String> {
    let refspec = format!("HEAD:{upstream_branch}");
    git_output(&["push"], flags, &[remote, &refspec])
}

pub fn pull(remote: &str, branch: &str, flags: &[&str]) -> Result<String> {
    git_output(&["pull"], flags, &[remote, branch])
}

fn git_output(base: &[&str], flags: &[&str], args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(base).args(flags).args(args)
        .output()
        .context(format!("failed to run git {}", base[0]))?;
    parse_output(&out)
}

fn git_output_with_file(base: &[&str], flags: &[&str], file: &std::path::Path) -> Result<String> {
    let out = std::process::Command::new("git")
        .args(base).args(flags).arg("-F").arg(file)
        .output()
        .context(format!("failed to run git {}", base[0]))?;
    parse_output(&out)
}

fn parse_output(out: &std::process::Output) -> Result<String> {
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let text = if !stderr.is_empty() { &stderr } else { &stdout };
    if !out.status.success() {
        let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("git command failed");
        anyhow::bail!("{}", first);
    }
    Ok(text.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("Done").to_string())
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
    opts.include_untracked(true).recurse_untracked_dirs(false);
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

pub fn commit(message: &str, flags: &[&str]) -> Result<String> {
    git_output_with_file(&["commit"], flags, &write_msg_file(message)?)
}

pub fn amend(message: &str, flags: &[&str]) -> Result<String> {
    git_output_with_file(&["commit", "--amend"], flags, &write_msg_file(message)?)
}

pub fn reword(message: &str, flags: &[&str]) -> Result<String> {
    git_output_with_file(&["commit", "--amend", "--only"], flags, &write_msg_file(message)?)
}

pub fn extend(flags: &[&str]) -> Result<String> {
    git_output(&["commit", "--amend", "--no-edit"], flags, &[])
}

fn write_msg_file(message: &str) -> Result<std::path::PathBuf> {
    let path = std::env::temp_dir().join("maguito_commit_msg");
    std::fs::write(&path, message).context("failed to write commit message")?;
    Ok(path)
}

pub fn commit_from(repo_path: &Path, message: &str) -> Result<()> {
    let repo = Repository::discover(repo_path)?;
    let sig = repo.signature().context("git user.name/email not configured")?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
    Ok(())
}

pub fn stage_file(path: &str) -> Result<()> {
    let out = std::process::Command::new("git")
        .current_dir(workdir()?)
        .args(["add", "--", path])
        .output()
        .context("failed to run git add")?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub fn unstage_file(path: &str) -> Result<()> {
    let out = std::process::Command::new("git")
        .current_dir(workdir()?)
        .args(["restore", "--staged", "--", path])
        .output()
        .context("failed to run git restore")?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

pub fn discard_file(path: &str, staged: bool) -> Result<()> {
    let args: &[&str] = if staged {
        &["checkout", "HEAD", "--", path]
    } else {
        &["checkout", "--", path]
    };
    let out = std::process::Command::new("git")
        .current_dir(workdir()?)
        .args(args)
        .output()
        .context("failed to run git checkout")?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

pub fn trash_file(path: &str) -> Result<()> {
    let p = workdir()?.join(path);
    if p.is_dir() {
        std::fs::remove_dir_all(p)?;
    } else {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

pub fn stage_hunk(file_path: &str, hunk: &Hunk) -> Result<()> {
    run_git_apply(&build_patch(file_path, hunk), false)
}

pub fn unstage_hunk(file_path: &str, hunk: &Hunk) -> Result<()> {
    run_git_apply(&build_patch(file_path, hunk), true)
}

pub fn discard_hunk(file_path: &str, hunk: &Hunk, staged: bool) -> Result<()> {
    let patch = build_patch(file_path, hunk);
    if staged {
        run_git_apply(&patch, true)          // --cached --reverse: remove from index
    } else {
        run_git_apply_worktree(&patch, true) // --reverse: restore working tree
    }
}

fn build_partial_patch(file_path: &str, hunk: &Hunk, selected: &[usize]) -> String {
    let sel: std::collections::HashSet<usize> = selected.iter().copied().collect();
    let mut body = String::new();
    let mut old_count = 0u32;
    let mut new_count = 0u32;

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.origin {
            ' ' => {
                body.push(' '); body.push_str(&line.content); body.push('\n');
                old_count += 1; new_count += 1;
            }
            '+' if sel.contains(&i) => {
                body.push('+'); body.push_str(&line.content); body.push('\n');
                new_count += 1;
            }
            '+' => {} // unselected addition: skip (stays unstaged)
            '-' if sel.contains(&i) => {
                body.push('-'); body.push_str(&line.content); body.push('\n');
                old_count += 1;
            }
            '-' => {
                // unselected deletion: treat as context (kept in index)
                body.push(' '); body.push_str(&line.content); body.push('\n');
                old_count += 1; new_count += 1;
            }
            _ => {}
        }
    }

    let mut s = String::new();
    s.push_str(&format!("diff --git a/{file_path} b/{file_path}\n"));
    s.push_str(&format!("--- a/{file_path}\n"));
    s.push_str(&format!("+++ b/{file_path}\n"));
    s.push_str(&format!("@@ -{},{} +{},{} @@\n", hunk.old_start, old_count, hunk.new_start, new_count));
    s.push_str(&body);
    s
}

pub fn stage_lines(file_path: &str, hunk: &Hunk, selected: &[usize]) -> Result<()> {
    let patch = build_partial_patch(file_path, hunk, selected);
    run_git_apply(&patch, false)
}

pub fn unstage_lines(file_path: &str, hunk: &Hunk, selected: &[usize]) -> Result<()> {
    let patch = build_partial_patch(file_path, hunk, selected);
    run_git_apply(&patch, true)
}

pub fn discard_lines(file_path: &str, hunk: &Hunk, selected: &[usize], staged: bool) -> Result<()> {
    let patch = build_partial_patch(file_path, hunk, selected);
    if staged {
        run_git_apply(&patch, true)?;
    }
    run_git_apply_worktree(&patch, true)
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
    git_apply(patch, &["--cached"], reverse)
}

fn run_git_apply_worktree(patch: &str, reverse: bool) -> Result<()> {
    git_apply(patch, &[], reverse)
}

fn git_apply(patch: &str, extra: &[&str], reverse: bool) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut args = vec!["apply"];
    args.extend_from_slice(extra);
    if reverse { args.push("--reverse"); }
    args.push("-");

    let mut child = Command::new("git")
        .current_dir(workdir()?)
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
