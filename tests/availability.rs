#[test]
fn descriptor_only_advertises_native_provider_platforms() {
    let manifest: serde_json::Value =
        serde_json::from_str(nddev_device_sync_gds::MANIFEST).unwrap();
    assert_eq!(manifest["platforms"], serde_json::json!(["linux", "macos"]));
    assert_eq!(
        manifest["permissions"],
        serde_json::json!(["read_only", "process_execution"])
    );
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn windows_compatibility_api_refuses_native_execution() {
    use nddev_device_sync_adapter_io::{CancellationToken, IoError, NativeIo};
    use nddev_device_sync_gds::Gds;
    let directory = tempfile::tempdir().unwrap();
    let provider = Gds::new(
        NativeIo::new(1).unwrap().process_handle(),
        std::env::current_exe().unwrap(),
    )
    .unwrap();
    assert!(matches!(
        provider
            .status(directory.path(), CancellationToken::new())
            .await,
        Err(IoError::Unavailable)
    ));
}
