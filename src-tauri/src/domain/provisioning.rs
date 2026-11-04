//! Provisioning decisions are pure: discovery and privileged execution live elsewhere.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StepStatus {
    Required,
    Satisfied,
    Unsupported,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub key: String,
    pub status: StepStatus,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProvisioningFacts {
    pub os_id: String,
    pub version_id: String,
    pub architecture: String,
    pub systemd: bool,
    pub apt: bool,
    pub administrative_access: bool,
    pub repository_reachable: bool,
    #[ts(type = "number")]
    pub available_bytes: u64,
    pub conflicting_packages: bool,
    pub custom_repository: bool,
    pub docker_state_known: bool,
    pub engine_installed: bool,
    pub compose_installed: bool,
    pub additional_packages_required: bool,
    pub service_running: bool,
    pub service_enabled: bool,
    pub repository_valid: bool,
}

pub const MINIMUM_INSTALL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn compatible(facts: &ProvisioningFacts) -> bool {
    facts.os_id == "ubuntu"
        && facts.version_id == "24.04"
        && matches!(facts.architecture.as_str(), "amd64" | "arm64")
}

/// Unknown remote state always blocks installation; history is not evidence.
pub fn build_plan(facts: &ProvisioningFacts) -> Vec<PlanStep> {
    let checks = [
        (
            "compatibility",
            compatible(facts),
            "Ubuntu 24.04 on amd64 or arm64 is required.",
        ),
        (
            "system",
            facts.systemd && facts.apt,
            "Running systemd and apt are required.",
        ),
        (
            "privileges",
            facts.administrative_access,
            "Root or passwordless sudo is required.",
        ),
        (
            "docker_detection",
            facts.docker_state_known,
            "Docker state must be established before changes.",
        ),
        (
            "conflicts",
            !facts.conflicting_packages
                && (!(!facts.engine_installed
                    || !facts.compose_installed
                    || facts.additional_packages_required)
                    || !facts.custom_repository),
            "Resolve conflicting packages or custom repositories manually.",
        ),
    ];
    let mut steps: Vec<_> = checks
        .into_iter()
        .map(|(key, passed, description)| PlanStep {
            key: key.into(),
            status: if passed {
                StepStatus::Satisfied
            } else if key == "compatibility" {
                StepStatus::Unsupported
            } else {
                StepStatus::Blocked
            },
            description: description.into(),
        })
        .collect();
    let packages_needed =
        !facts.engine_installed || !facts.compose_installed || facts.additional_packages_required;
    if packages_needed {
        for (key, passed, description) in [
            (
                "disk",
                facts.available_bytes >= MINIMUM_INSTALL_BYTES,
                "At least 2 GiB available space is required.",
            ),
            (
                "connectivity",
                facts.repository_reachable,
                "Package repositories must be reachable.",
            ),
        ] {
            steps.push(PlanStep {
                key: key.into(),
                status: if passed {
                    StepStatus::Satisfied
                } else {
                    StepStatus::Blocked
                },
                description: description.into(),
            });
        }
    }
    let blocked = steps
        .iter()
        .any(|s| matches!(s.status, StepStatus::Blocked | StepStatus::Unsupported));
    for (key, satisfied, description) in [
        (
            "repository",
            !packages_needed || facts.repository_valid,
            "Configure the signed official Docker repository.",
        ),
        (
            "engine",
            facts.engine_installed,
            "Install missing Docker Engine packages.",
        ),
        (
            "compose",
            facts.compose_installed,
            "Install Docker Compose plugin.",
        ),
        (
            "buildx",
            !facts.additional_packages_required,
            "Install the missing Docker Buildx plugin.",
        ),
        ("enable", facts.service_enabled, "Enable Docker at boot."),
        (
            "start",
            facts.service_running,
            "Start Docker without restarting an active daemon.",
        ),
    ] {
        steps.push(PlanStep {
            key: key.into(),
            status: if satisfied {
                StepStatus::Satisfied
            } else if blocked {
                StepStatus::Blocked
            } else {
                StepStatus::Required
            },
            description: description.into(),
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_state_cannot_authorize_changes() {
        assert!(!build_plan(&ProvisioningFacts::default())
            .iter()
            .any(|s| s.status == StepStatus::Required));
    }
    #[test]
    fn rejects_derivatives_and_other_versions() {
        let mut facts = ProvisioningFacts {
            os_id: "ubuntu".into(),
            version_id: "24.04".into(),
            architecture: "arm64".into(),
            ..Default::default()
        };
        assert!(compatible(&facts));
        facts.os_id = "linuxmint".into();
        assert!(!compatible(&facts));
        facts.os_id = "ubuntu".into();
        facts.version_id = "22.04".into();
        assert!(!compatible(&facts));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub facts: ProvisioningFacts,
    pub checks: Vec<PlanStep>,
    pub username: String,
    pub privilege: String,
    pub codename: String,
    pub kernel: String,
    pub docker_source: String,
    pub client_version: String,
    pub server_version: String,
    pub compose_version: String,
    pub docker_access: String,
    pub docker_group: bool,
    pub pending_login: bool,
    pub workloads: Option<u32>,
    pub checked_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProvisioningPlan {
    pub id: String,
    pub server_id: String,
    pub plan_hash: String,
    pub expires_at: String,
    pub preflight: Preflight,
    pub steps: Vec<PlanStep>,
    pub grant_access: bool,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunStatus {
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    PartiallyCompleted,
    Interrupted,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RunStep {
    pub key: String,
    pub description: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error_code: Option<String>,
    pub sanitized_output: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProvisioningRun {
    #[serde(default)]
    pub approved_plan: Option<ProvisioningPlan>,
    #[serde(default)]
    pub logs: Vec<ProvisioningLog>,
    pub id: String,
    pub server_id: String,
    pub plan_hash: String,
    pub status: RunStatus,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub cancellation_requested: bool,
    pub sequence: u32,
    pub steps: Vec<RunStep>,
    pub verification: Option<Preflight>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProvisioningLog {
    pub id: String,
    pub step_key: String,
    pub observed_at: String,
    pub level: String,
    pub message: String,
}

#[cfg(test)]
mod history_contract_tests {
    use super::*;

    #[test]
    fn legacy_snapshot_loads_without_new_history_fields() {
        let snapshot = serde_json::json!({
            "id":"run", "serverId":"server", "planHash":"hash", "status":"INTERRUPTED",
            "startedAt":"2026-11-04T00:00:00Z", "finishedAt":null,
            "errorCode":null, "errorMessage":null, "cancellationRequested":false,
            "sequence":1, "steps":[], "verification":null
        });
        let mut run: ProvisioningRun = serde_json::from_value(snapshot).expect("legacy snapshot");
        assert!(run.approved_plan.is_none());
        assert!(run.logs.is_empty());
        run.logs.push(ProvisioningLog {
            id: "entry".into(),
            step_key: "engine".into(),
            observed_at: "2026-11-04T00:00:00Z".into(),
            level: "INFO".into(),
            message: "Unpacking docker-ce".into(),
        });
        let restored: ProvisioningRun =
            serde_json::from_str(&serde_json::to_string(&run).expect("serialize"))
                .expect("restore");
        assert_eq!(restored.logs[0].id, "entry");
        assert_eq!(restored.logs[0].message, "Unpacking docker-ce");
    }
}
