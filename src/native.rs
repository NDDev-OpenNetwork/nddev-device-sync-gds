use crate::response::Envelope;
use crate::view::*;
use nddev_device_sync_adapter_io::{CancellationToken, IoError, ProcessIo, ProcessRequest};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct Gds {
    io: ProcessIo,
    executable: PathBuf,
}

impl Gds {
    pub fn new(io: ProcessIo, executable: PathBuf) -> Result<Self, IoError> {
        if !executable.is_absolute() {
            return Err(IoError::InvalidConfiguration);
        }
        Ok(Self { io, executable })
    }

    pub async fn status(
        &self,
        repository: &Path,
        cancellation: CancellationToken,
    ) -> Result<Status, IoError> {
        if !repository.is_absolute() {
            return Err(IoError::InvalidConfiguration);
        }
        // Preserve the existing Windows Rust API for consumers, but the pinned
        // native provider's trusted Git discovery supports only Linux/macOS.
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            return Err(IoError::Unavailable);
        }
        let request = ProcessRequest {
            executable: self.executable.clone(),
            arguments: vec![
                "status".into(),
                "--cwd".into(),
                repository.as_os_str().into(),
                "--json".into(),
                "--timeout".into(),
                "10s".into(),
            ],
            directory: Some(repository.into()),
            timeout: Duration::from_secs(12),
            max_output_bytes: 1_048_576,
        };
        let (envelope, exit): (Envelope, _) = self
            .io
            .json(
                request,
                &[0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
                cancellation,
            )
            .await?;
        envelope
            .into_status(exit)
            .map_err(|_| IoError::InvalidResponse)
    }
}
