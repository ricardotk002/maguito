use std::collections::HashSet;

pub struct FlagDef {
    pub key: char,
    pub short: &'static str,
    pub label: &'static str,
    pub git_flag: &'static str,
}

pub struct ActionDef {
    pub key: char,
    pub label: &'static str,
}

pub const COMMIT_FLAGS: &[FlagDef] = &[
    FlagDef { key: 'a', short: "-a", label: "Stage all modified and deleted files", git_flag: "--all" },
    FlagDef { key: 'e', short: "-e", label: "Allow empty commit",                   git_flag: "--allow-empty" },
    FlagDef { key: 'v', short: "-v", label: "Show diff of changes to be committed", git_flag: "--verbose" },
    FlagDef { key: 'n', short: "-n", label: "Disable hooks",                        git_flag: "--no-verify" },
    FlagDef { key: 's', short: "-s", label: "Add Signed-off-by line",               git_flag: "--signoff" },
];

pub const COMMIT_CREATE: &[ActionDef] = &[
    ActionDef { key: 'c', label: "Commit" },
];

pub const COMMIT_EDIT_HEAD: &[ActionDef] = &[
    ActionDef { key: 'e', label: "Extend" },
    ActionDef { key: 'a', label: "Amend" },
    ActionDef { key: 'w', label: "Reword" },
];

pub const PUSH_FLAGS: &[FlagDef] = &[
    FlagDef { key: 'f', short: "-f", label: "Force with lease", git_flag: "--force-with-lease" },
    FlagDef { key: 'F', short: "-F", label: "Force",            git_flag: "--force" },
    FlagDef { key: 'n', short: "-n", label: "Dry run",          git_flag: "--dry-run" },
    FlagDef { key: 'u', short: "-u", label: "Set upstream",     git_flag: "--set-upstream" },
];

pub const PUSH_ACTIONS: &[ActionDef] = &[
    ActionDef { key: 'p', label: "push-remote" },
    ActionDef { key: 'u', label: "upstream" },
];

pub const PULL_FLAGS: &[FlagDef] = &[
    FlagDef { key: 'f', short: "-f", label: "Fast-forward only", git_flag: "--ff-only" },
    FlagDef { key: 'r', short: "-r", label: "Rebase",            git_flag: "--rebase" },
    FlagDef { key: 'A', short: "-A", label: "Autostash",         git_flag: "--autostash" },
];

pub const PULL_ACTIONS: &[ActionDef] = &[
    ActionDef { key: 'p', label: "push-remote" },
    ActionDef { key: 'u', label: "upstream" },
];

pub const FETCH_FLAGS: &[FlagDef] = &[
    FlagDef { key: 'p', short: "-p", label: "Prune deleted branches", git_flag: "--prune" },
    FlagDef { key: 't', short: "-t", label: "Fetch all tags",         git_flag: "--tags" },
    FlagDef { key: 'F', short: "-F", label: "Force",                  git_flag: "--force" },
];

pub const FETCH_ACTIONS: &[ActionDef] = &[
    ActionDef { key: 'p', label: "push-remote" },
    ActionDef { key: 'u', label: "upstream" },
    ActionDef { key: 'a', label: "all remotes" },
];

#[derive(Copy, Clone)]
pub enum TransientKind {
    Commit,
    Fetch,
    Push,
    Pull,
}

pub struct Transient {
    pub kind: TransientKind,
    pub active_flags: HashSet<&'static str>,
    pub awaiting_flag: bool,
}

impl Transient {
    pub fn commit() -> Self {
        Self { kind: TransientKind::Commit, active_flags: HashSet::new(), awaiting_flag: false }
    }
    pub fn fetch() -> Self {
        Self { kind: TransientKind::Fetch, active_flags: HashSet::new(), awaiting_flag: false }
    }
    pub fn push() -> Self {
        Self { kind: TransientKind::Push, active_flags: HashSet::new(), awaiting_flag: false }
    }
    pub fn pull() -> Self {
        Self { kind: TransientKind::Pull, active_flags: HashSet::new(), awaiting_flag: false }
    }

    pub fn flags_vec(&self) -> Vec<&'static str> {
        self.flag_defs().iter()
            .filter(|f| self.active_flags.contains(f.git_flag))
            .map(|f| f.git_flag)
            .collect()
    }

    pub fn toggle_flag(&mut self, key: char) -> bool {
        if let Some(f) = self.flag_defs().iter().find(|f| f.key == key) {
            if self.active_flags.contains(f.git_flag) {
                self.active_flags.remove(f.git_flag);
            } else {
                self.active_flags.insert(f.git_flag);
            }
            true
        } else {
            false
        }
    }

    fn flag_defs(&self) -> &'static [FlagDef] {
        match self.kind {
            TransientKind::Commit => COMMIT_FLAGS,
            TransientKind::Fetch  => FETCH_FLAGS,
            TransientKind::Push   => PUSH_FLAGS,
            TransientKind::Pull   => PULL_FLAGS,
        }
    }
}
