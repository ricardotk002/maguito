use anyhow::{Context, Result};
use git2::{Repository, Status, StatusOptions};

#[derive(Debug, Default)]
pub struct RepoStatus {
    pub branch: String,
    pub untracked: Vec<String>,
    pub unstaged: Vec<String>,
    pub staged: Vec<String>,
}

pub fn load() -> Result<RepoStatus> {
    let repo = Repository::discover(".").context("not a git repository")?;
    let mut status = RepoStatus::default();

    status.branch = match repo.head() {
        Ok(head) => head.shorthand().unwrap_or("HEAD").to_string(),
        Err(_) => "(no commits yet)".to_string(),
    };

    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut opts))?;

    for entry in statuses.iter() {
        let path = entry.path().unwrap_or("").to_string();
        let s = entry.status();

        if s.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED,
        ) {
            status.staged.push(path.clone());
        }
        if s.intersects(Status::WT_MODIFIED | Status::WT_DELETED) {
            status.unstaged.push(path.clone());
        }
        if s.contains(Status::WT_NEW) {
            status.untracked.push(path);
        }
    }

    Ok(status)
}
