use crate::view::*;
use nddev_device_sync_adapter_io::{CancellationToken, IoError, ProcessIo, ProcessRequest};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
struct Envelope {
    schema_version: u8,
    command: String,
    result: String,
    exit_class: String,
    exit_code: i32,
    data: Option<StatusData>,
    findings: Vec<Finding>,
    mutation: Mutation,
}

#[derive(Deserialize)]
struct StatusData {
    status: Option<GitStatus>,
}
#[derive(Deserialize)]
struct Mutation {
    attempted: bool,
    completed: bool,
}

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
        if envelope.schema_version != 1
            || envelope.command != "gds status"
            || envelope.exit_code != exit
            || envelope.mutation.attempted
            || envelope.mutation.completed
            || envelope.findings.len() > 256
            || expected_exit(&envelope.exit_class) != Some(exit)
            || expected_result(&envelope.exit_class) != envelope.result
        {
            return Err(IoError::InvalidResponse);
        }
        Ok(Status {
            result: envelope.result,
            exit_class: envelope.exit_class,
            status: envelope.data.and_then(|data| data.status),
            findings: envelope.findings,
        })
    }
}

fn expected_exit(class: &str) -> Option<i32> {
    Some(match class {
        "success" => 0,
        "validation" => 2,
        "not-proven" => 3,
        "input" => 4,
        "stale" => 5,
        "approval" => 6,
        "authorization" => 7,
        "conflict" => 8,
        "policy" => 9,
        "partial" => 10,
        "provider-transient" => 11,
        "security" => 12,
        "unsupported" => 13,
        "internal" => 14,
        _ => return None,
    })
}

fn expected_result(class: &str) -> &'static str {
    match class {
        "success" => "succeeded",
        "not-proven" => "not-proven",
        "partial" => "partial",
        "approval" | "authorization" | "conflict" | "policy" | "security" | "stale"
        | "unsupported" => "blocked",
        _ => "failed",
    }
}
