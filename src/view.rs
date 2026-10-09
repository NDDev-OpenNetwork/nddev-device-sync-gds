//! Portable native-provider response projections. No I/O or UI runtime.
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Changes {
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub submodule_changes: u32,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Branch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub upstream: Option<String>,
    pub upstream_state: String,
    pub comparison_state: String,
    pub ahead: u32,
    pub behind: u32,
    pub diverged: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct GitStatus {
    pub branch: Branch,
    pub changes: Changes,
    pub classification: String,
    pub remote_freshness: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub result: String,
    pub exit_class: String,
    pub status: Option<GitStatus>,
    pub findings: Vec<Finding>,
}
