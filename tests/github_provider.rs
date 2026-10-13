#![cfg(all(not(target_arch = "wasm32"), feature = "github-catalog"))]
use nddev_device_sync_domain::{
    modules::ModuleError,
    repositories::catalog::{CatalogPageRequest, CredentialSelection},
};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
use tracing::instrument::WithSubscriber;
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[tokio::test]
#[ignore = "actual fixed GitHub negative; generated invalid credential only, no positive fixture"]
async fn fixed_github_rejects_generated_invalid_credential_without_empty_success() {
    let request = CatalogPageRequest::new(
        CredentialSelection::new("generated-invalid-fixture".into(), 0, None).unwrap(),
        None,
    )
    .unwrap();
    let token = format!(
        "nds_generated_invalid_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let captured = Capture(Arc::default());
    let sink = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || sink.clone())
        .finish();
    let result = nddev_device_sync_gds::github::read_user_and_page(
        &token,
        &request,
        &Default::default(),
        async || Ok(()),
    )
    .with_subscriber(subscriber)
    .await;
    assert!(matches!(result, Err(ModuleError::Unavailable)));
    let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("http.response.status_code=401"));
    assert!(text.contains("gds.catalog.read.failed"));
    assert!(
        !text.contains(&token)
            && !text.contains("Bearer ")
            && !text.contains("https://api.github.com")
    );
}
