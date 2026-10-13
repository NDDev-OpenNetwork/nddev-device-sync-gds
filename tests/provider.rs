#![cfg(any(target_os = "linux", target_os = "macos"))]

use nddev_device_sync_adapter_io::{CancellationToken, NativeIo};
use nddev_device_sync_gds::Gds;
use std::{path::Path, process::Command};

fn git(repository: &Path, arguments: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let output = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.name=NDS synthetic acceptance",
            "-c",
            "user.email=acceptance@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "core.autocrlf=false",
        ])
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated Git fixture command failed"
    );
    output.stdout
}

fn provider() -> Gds {
    let executable = std::env::var_os("NDS_TEST_GDS").expect("native GDS required");
    Gds::new(
        NativeIo::new(1).unwrap().process_handle(),
        executable.into(),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "requires NDS_TEST_GDS; real native inspection of an isolated Git repository"]
async fn preserves_dirty_work_index_and_refs_without_claiming_remote_freshness() {
    let repository = tempfile::tempdir().unwrap();
    let root = repository.path();
    git(root, &["init", "--quiet", "--template="]);
    std::fs::write(root.join("tracked.txt"), "initial\n").unwrap();
    git(root, &["add", "--", "tracked.txt"]);
    git(root, &["commit", "--quiet", "-m", "synthetic baseline"]);
    std::fs::write(root.join("staged.txt"), "index content\n").unwrap();
    git(root, &["add", "--", "staged.txt"]);
    std::fs::write(root.join("staged.txt"), "different worktree content\n").unwrap();
    std::fs::write(root.join("untracked.txt"), "preserve untracked content\n").unwrap();
    let refs = git(root, &["show-ref", "--head"]);
    let index = git(root, &["ls-files", "--stage", "-z"]);
    let staged = git(root, &["show", ":staged.txt"]);
    let files = ["tracked.txt", "staged.txt", "untracked.txt"]
        .map(|name| std::fs::read(root.join(name)).unwrap());

    let report = provider()
        .status(root, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.result, "not-proven");
    assert_eq!(report.exit_class, "not-proven");
    assert!(!report.findings.is_empty());
    let status = report.status.expect("real Git status required");
    assert_eq!(status.remote_freshness, "unknown");
    assert_eq!(status.classification, "dirty");
    assert_eq!(status.changes.staged, 1);
    assert_eq!(status.changes.unstaged, 1);
    assert_eq!(status.changes.untracked, 1);
    // Compare semantic index content; a native read may refresh stat caches.
    assert_eq!(git(root, &["ls-files", "--stage", "-z"]), index);
    assert_eq!(git(root, &["show", ":staged.txt"]), staged);
    assert_eq!(git(root, &["show-ref", "--head"]), refs);
    for (name, content) in ["tracked.txt", "staged.txt", "untracked.txt"]
        .into_iter()
        .zip(files)
    {
        assert_eq!(std::fs::read(root.join(name)).unwrap(), content);
    }
}

#[tokio::test]
#[ignore = "requires NDS_TEST_GDS; real native inspection of an isolated unborn repository"]
async fn preserves_unborn_status_without_creating_a_commit() {
    let repository = tempfile::tempdir().unwrap();
    let root = repository.path();
    git(root, &["init", "--quiet", "--template="]);
    std::fs::write(root.join("staged.txt"), "synthetic content\n").unwrap();
    git(root, &["add", "--", "staged.txt"]);
    let index = git(root, &["ls-files", "--stage", "-z"]);
    let report = provider()
        .status(root, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.result, "not-proven");
    let status = report.status.unwrap();
    assert_eq!(status.classification, "unborn");
    assert_eq!(status.branch.upstream_state, "not-applicable");
    assert_eq!(status.remote_freshness, "unknown");
    assert_eq!(git(root, &["ls-files", "--stage", "-z"]), index);
    assert!(git(root, &["for-each-ref", "--format=%(refname)"]).is_empty());
    assert_eq!(
        std::fs::read(root.join("staged.txt")).unwrap(),
        b"synthetic content\n"
    );
}

#[tokio::test]
#[ignore = "requires NDS_TEST_GDS; real native refusal for a non-Git directory"]
async fn missing_git_evidence_is_not_a_successful_empty_status() {
    let directory = tempfile::tempdir().unwrap();
    let report = provider()
        .status(directory.path(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.result, "not-proven");
    assert_eq!(report.exit_class, "not-proven");
    assert!(report.status.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "GDS_GIT_STATUS_NOT_PROVEN")
    );
    assert!(!directory.path().join(".git").exists());
}
