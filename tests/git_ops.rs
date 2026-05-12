use std::fs;
use std::path::Path;
use git2::{Repository, Signature};
use tempfile::TempDir;
use maguito::git::repo::{self, SectionKind};

// ── helpers ──────────────────────────────────────────────────────────────────

struct TestRepo {
    _dir: TempDir, // keeps the dir alive
    pub repo: Repository,
    pub path: std::path::PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        let repo = Repository::init(&path).unwrap();
        {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test").unwrap();
            cfg.set_str("user.email", "test@test.com").unwrap();
        }
        Self { _dir: dir, repo, path }
    }

    fn sig(&self) -> Signature<'static> {
        Signature::now("Test", "test@test.com").unwrap()
    }

    fn write(&self, name: &str, content: &str) {
        fs::write(self.path.join(name), content).unwrap();
    }

    fn stage(&self, name: &str) {
        let mut index = self.repo.index().unwrap();
        index.add_path(Path::new(name)).unwrap();
        index.write().unwrap();
    }

    fn commit(&self, message: &str) {
        let sig = self.sig();
        let mut index = self.repo.index().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = self.repo.find_tree(tree_id).unwrap();
        let parents: Vec<git2::Commit> = self.repo.head().ok()
            .and_then(|h| h.peel_to_commit().ok())
            .into_iter()
            .collect();
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        self.repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs).unwrap();
    }
}

// ── load tests ───────────────────────────────────────────────────────────────

#[test]
fn load_detects_branch_name() {
    let tr = TestRepo::new();
    tr.write("init.txt", "hello");
    tr.stage("init.txt");
    tr.commit("initial");

    let status = repo::load_from(&tr.path).unwrap();
    assert_eq!(status.branch, "master");
}

#[test]
fn load_detects_untracked_files() {
    let tr = TestRepo::new();
    tr.write("init.txt", "hello");
    tr.stage("init.txt");
    tr.commit("initial");

    tr.write("new.txt", "untracked");

    let status = repo::load_from(&tr.path).unwrap();
    let untracked = status.sections.iter()
        .find(|(k, _)| *k == SectionKind::Untracked);
    assert!(untracked.is_some());
    assert!(untracked.unwrap().1.iter().any(|f| f.path == "new.txt"));
}

#[test]
fn load_detects_staged_new_file() {
    let tr = TestRepo::new();
    tr.write("init.txt", "hello");
    tr.stage("init.txt");
    tr.commit("initial");

    tr.write("staged.txt", "content");
    tr.stage("staged.txt");

    let status = repo::load_from(&tr.path).unwrap();
    let staged = status.sections.iter().find(|(k, _)| *k == SectionKind::Staged);
    assert!(staged.is_some());
    assert!(staged.unwrap().1.iter().any(|f| f.path == "staged.txt"));
}

#[test]
fn load_detects_unstaged_modification() {
    let tr = TestRepo::new();
    tr.write("file.txt", "original\n");
    tr.stage("file.txt");
    tr.commit("initial");

    tr.write("file.txt", "modified\n");

    let status = repo::load_from(&tr.path).unwrap();
    let unstaged = status.sections.iter().find(|(k, _)| *k == SectionKind::Unstaged);
    assert!(unstaged.is_some());
    assert!(unstaged.unwrap().1.iter().any(|f| f.path == "file.txt"));
}

#[test]
fn load_returns_recent_commits() {
    let tr = TestRepo::new();
    tr.write("a.txt", "a");
    tr.stage("a.txt");
    tr.commit("first commit");

    tr.write("b.txt", "b");
    tr.stage("b.txt");
    tr.commit("second commit");

    let status = repo::load_from(&tr.path).unwrap();
    assert_eq!(status.commits.len(), 2);
    assert_eq!(status.commits[0].message, "second commit");
    assert_eq!(status.commits[1].message, "first commit");
}

#[test]
fn staged_section_appears_before_unstaged() {
    let tr = TestRepo::new();
    tr.write("file.txt", "line1\nline2\n");
    tr.stage("file.txt");
    tr.commit("initial");

    // unstaged modification
    tr.write("file.txt", "line1\nline2\nline3\n");
    // staged new file
    tr.write("new.txt", "new");
    tr.stage("new.txt");

    let status = repo::load_from(&tr.path).unwrap();
    let kinds: Vec<&SectionKind> = status.sections.iter().map(|(k, _)| k).collect();
    let staged_pos   = kinds.iter().position(|k| **k == SectionKind::Staged);
    let unstaged_pos = kinds.iter().position(|k| **k == SectionKind::Unstaged);
    assert!(staged_pos.unwrap() < unstaged_pos.unwrap());
}
