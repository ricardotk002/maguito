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

pub enum TransientKind {
    Commit,
}

pub struct Transient {
    pub kind: TransientKind,
    pub active_flags: HashSet<&'static str>,
    pub awaiting_flag: bool,
}

impl Transient {
    pub fn commit() -> Self {
        Self {
            kind: TransientKind::Commit,
            active_flags: HashSet::new(),
            awaiting_flag: false,
        }
    }

    pub fn flags_vec(&self) -> Vec<&'static str> {
        COMMIT_FLAGS.iter()
            .filter(|f| self.active_flags.contains(f.git_flag))
            .map(|f| f.git_flag)
            .collect()
    }

    pub fn toggle_flag(&mut self, key: char) -> bool {
        if let Some(f) = COMMIT_FLAGS.iter().find(|f| f.key == key) {
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
}
