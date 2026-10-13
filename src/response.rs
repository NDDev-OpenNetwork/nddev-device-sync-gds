//! Private decoder for the pinned GDS v1 status wire contract. This validates
//! evidence shape; Git classification and authorization remain native-owned.
use crate::view::{Finding, GitStatus, Status};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Envelope {
    schema_version: u8,
    command: String,
    result: Outcome,
    exit_class: ExitClass,
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

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    Succeeded,
    NotProven,
    Blocked,
    Partial,
    Failed,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::NotProven => "not-proven",
            Self::Blocked => "blocked",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ExitClass {
    Success,
    Validation,
    NotProven,
    Input,
    Stale,
    Approval,
    Authorization,
    Conflict,
    Policy,
    Partial,
    ProviderTransient,
    Security,
    Unsupported,
    Internal,
}

impl ExitClass {
    // Native core/domain/result.go at provider.lock.json's gds-v0.9.27.
    fn contract(self) -> (i32, Outcome, &'static str) {
        use Outcome::*;
        match self {
            Self::Success => (0, Succeeded, "success"),
            Self::Validation => (2, Failed, "validation"),
            Self::NotProven => (3, NotProven, "not-proven"),
            Self::Input => (4, Failed, "input"),
            Self::Stale => (5, Blocked, "stale"),
            Self::Approval => (6, Blocked, "approval"),
            Self::Authorization => (7, Blocked, "authorization"),
            Self::Conflict => (8, Blocked, "conflict"),
            Self::Policy => (9, Blocked, "policy"),
            Self::Partial => (10, Partial, "partial"),
            Self::ProviderTransient => (11, Failed, "provider-transient"),
            Self::Security => (12, Blocked, "security"),
            Self::Unsupported => (13, Blocked, "unsupported"),
            Self::Internal => (14, Failed, "internal"),
        }
    }
}

#[derive(Debug)]
pub(super) struct InvalidResponse;

impl Envelope {
    pub(super) fn into_status(self, process_exit: i32) -> Result<Status, InvalidResponse> {
        let (expected_exit, expected_outcome, class) = self.exit_class.contract();
        let status = self.data.and_then(|data| data.status);
        if self.schema_version != 1
            || self.command != "gds status"
            || self.exit_code != process_exit
            || expected_exit != process_exit
            || expected_outcome != self.result
            || self.mutation.attempted
            || self.mutation.completed
            || self.findings.len() > 256
            || (self.result == Outcome::Succeeded && status.is_none())
            || (self.result != Outcome::Succeeded && self.findings.is_empty())
            || self.findings.iter().any(|finding| {
                finding.code.trim().is_empty()
                    || finding.message.trim().is_empty()
                    || !matches!(
                        finding.severity.as_str(),
                        "info" | "low" | "medium" | "high" | "critical"
                    )
            })
            || status.as_ref().is_some_and(|value| !valid_status(value))
        {
            return Err(InvalidResponse);
        }
        Ok(Status {
            result: self.result.as_str().into(),
            exit_class: class.into(),
            status,
            findings: self.findings,
        })
    }
}

fn valid_status(status: &GitStatus) -> bool {
    // This command does not refresh refs or read durable refresh evidence.
    // Even "succeeded" + "clean-cached" cannot establish remote consistency.
    status.remote_freshness == "unknown"
        && matches!(
            status.classification.as_str(),
            "conflicted" | "unborn" | "detached" | "dirty" | "diverged-cached"
                | "ahead-cached" | "behind-cached" | "no-upstream" | "clean-cached"
        )
        && matches!(
            status.branch.upstream_state.as_str(),
            "present" | "missing" | "not-applicable"
        )
        // Native porcelain parsing can leave comparison empty when no branch.ab
        // record was provided. Preserve that lack of evidence, never infer zero.
        && matches!(
            status.branch.comparison_state.as_str(),
            "available" | "unavailable" | "not-applicable" | ""
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn response() -> Value {
        json!({
            "schema_version": 1, "command": "gds status", "result": "succeeded",
            "exit_class": "success", "exit_code": 0, "findings": [],
            "mutation": {"attempted": false, "completed": false},
            "data": {"status": {
                "branch": {"name": "main", "upstream": "origin/main", "upstream_state": "present",
                    "comparison_state": "available", "ahead": 0, "behind": 0, "diverged": false},
                "changes": {"staged": 0, "unstaged": 0, "untracked": 0, "conflicted": 0, "submodule_changes": 0},
                "classification": "clean-cached", "remote_freshness": "unknown"
            }}
        })
    }

    fn decode(value: Value, exit: i32) -> Result<Status, ()> {
        serde_json::from_value::<Envelope>(value)
            .map_err(|_| ())?
            .into_status(exit)
            .map_err(|_| ())
    }

    fn finding() -> Value {
        json!({"code": "GDS_CONTEXT_BUNDLE_MISSING", "severity": "medium", "message": "Policy proof is missing."})
    }

    #[test]
    fn successful_local_query_retains_unknown_remote_proof() {
        let report = decode(response(), 0).unwrap();
        assert_eq!(report.result, "succeeded");
        let status = report.status.unwrap();
        assert_eq!(status.classification, "clean-cached");
        assert_eq!(status.remote_freshness, "unknown");
    }

    #[test]
    fn preserves_unproven_blocked_partial_and_failed_observations() {
        for (class, outcome, exit) in [
            ("not-proven", "not-proven", 3),
            ("policy", "blocked", 9),
            ("stale", "blocked", 5),
            ("approval", "blocked", 6),
            ("authorization", "blocked", 7),
            ("conflict", "blocked", 8),
            ("security", "blocked", 12),
            ("unsupported", "blocked", 13),
            ("partial", "partial", 10),
            ("validation", "failed", 2),
            ("input", "failed", 4),
            ("provider-transient", "failed", 11),
            ("internal", "failed", 14),
        ] {
            let mut value = response();
            value["exit_class"] = class.into();
            value["result"] = outcome.into();
            value["exit_code"] = exit.into();
            value["findings"] = json!([finding()]);
            let report = decode(value.clone(), exit).unwrap();
            assert_eq!(report.result, outcome);
            assert_eq!(report.exit_class, class);
            assert!(report.status.is_some());
            value["data"] = json!({});
            assert!(decode(value.clone(), exit).unwrap().status.is_none());
            value["result"] = "succeeded".into();
            assert!(decode(value, exit).is_err());
        }
    }

    #[test]
    fn rejects_missing_success_evidence_and_unexplained_non_success() {
        for data in [Value::Null, json!({}), json!({"status": null})] {
            let mut value = response();
            value["data"] = data;
            assert!(decode(value, 0).is_err());
        }
        let mut value = response();
        value["exit_class"] = "not-proven".into();
        value["result"] = "not-proven".into();
        value["exit_code"] = 3.into();
        assert!(decode(value, 3).is_err());
    }

    #[test]
    fn rejects_wrong_command_version_exit_and_mutation_evidence() {
        for (pointer, replacement) in [
            ("/schema_version", json!(2)),
            ("/command", json!("gds sync checkouts apply")),
            ("/exit_code", json!(3)),
            ("/exit_class", json!("future-class")),
            ("/result", json!("healthy")),
            ("/mutation/attempted", json!(true)),
            ("/mutation/completed", json!(true)),
        ] {
            let mut value = response();
            *value.pointer_mut(pointer).unwrap() = replacement;
            assert!(decode(value, 0).is_err(), "{pointer}");
        }
        assert!(decode(response(), 3).is_err());
    }

    #[test]
    fn rejects_impossible_freshness_and_unknown_status_vocabulary() {
        for (field, value) in [
            ("remote_freshness", "fresh"),
            ("classification", "clean-current"),
            ("classification", ""),
            ("branch/upstream_state", "healthy"),
            ("branch/comparison_state", "equal"),
        ] {
            let mut response = response();
            *response
                .pointer_mut(&format!("/data/status/{field}"))
                .unwrap() = value.into();
            assert!(decode(response, 0).is_err(), "{field}");
        }
        let mut response = response();
        response["data"]["status"]["changes"]["staged"] = json!(-1);
        assert!(decode(response, 0).is_err());
    }

    #[test]
    fn bounds_findings_and_rejects_malformed_findings() {
        let mut value = response();
        value["findings"] = json!(vec![finding(); 256]);
        assert!(decode(value.clone(), 0).is_ok());
        value["findings"].as_array_mut().unwrap().push(finding());
        assert!(decode(value, 0).is_err());
        for (field, replacement) in [("code", ""), ("severity", "ok"), ("message", " ")] {
            let mut value = response();
            value["findings"] = json!([finding()]);
            value["findings"][0][field] = replacement.into();
            assert!(decode(value, 0).is_err());
        }
    }

    #[test]
    fn preserves_provider_extensions_without_exposing_private_evidence() {
        let mut value = response();
        value["scope"] = json!({"future": "provider-owned"});
        value["findings"] = json!([finding()]);
        value["findings"][0]["evidence"] = json!({"private_marker": "synthetic-private"});
        let report = decode(value, 0).unwrap();
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("synthetic-private"));
        assert!(!serialized.contains("provider-owned"));
    }
}
