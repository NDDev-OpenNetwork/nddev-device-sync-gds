use nddev_device_sync_adapter_io::{CancellationToken, NativeIo};
use nddev_device_sync_gds::Gds;

#[tokio::test]
#[ignore = "requires NDS_TEST_GDS for a real native inspection of an isolated Git repository"]
async fn preserves_native_status_and_unknown_remote_freshness() {
    let executable = std::env::var_os("NDS_TEST_GDS").expect("native GDS required");
    let repository = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .arg(repository.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        repository.path().join("staged.txt"),
        "isolated native acceptance\n",
    )
    .unwrap();
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(repository.path())
            .args(["add", "--", "staged.txt"])
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(repository.path().join("untracked.txt"), "not committed\n").unwrap();
    let index_before = std::fs::read(repository.path().join(".git/index")).unwrap();
    let provider = Gds::new(
        NativeIo::new(1).unwrap().process_handle(),
        executable.into(),
    )
    .unwrap();
    let report = provider
        .status(repository.path(), CancellationToken::new())
        .await
        .unwrap();
    let status = report.status.expect("real Git status required");
    assert_eq!(status.remote_freshness, "unknown");
    assert!(!status.classification.is_empty());
    assert_eq!(status.changes.staged, 1);
    assert_eq!(status.changes.untracked, 1);
    // Native Git may refresh index stat cache during a status read. The adapter
    // never changes the staged content or creates a commit/ref.
    assert!(!index_before.is_empty());
    assert!(
        std::process::Command::new("git")
            .arg("-C")
            .arg(repository.path())
            .args(["diff", "--exit-code", "--", "staged.txt"])
            .status()
            .unwrap()
            .success()
    );
    if report.exit_class == "not-proven" {
        assert_eq!(report.result, "not-proven");
        assert!(!report.findings.is_empty());
    }
}
