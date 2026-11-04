use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    AuthenticationFailed,
    HostKeyUntrusted,
    HostKeyMismatch,
    Timeout,
    Unreachable,
    Error,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyObservation {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
    pub previous_fingerprint: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub status: ConnectionStatus,
    pub latency_ms: Option<f64>,
    pub message: String,
    pub error_code: Option<String>,
    pub tested_at: String,
    pub observation: Option<HostKeyObservation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub distribution: Option<String>,
    pub version: Option<String>,
    pub kernel: String,
    pub architecture: String,
    pub hostname: String,
    pub uptime_seconds: f64,
    pub logical_cpus: u32,
    pub memory_total_bytes: f64,
    pub memory_available_bytes: f64,
    pub disk_total_bytes: f64,
    pub disk_available_bytes: f64,
    pub load_average: Vec<f64>,
    pub provisioning_supported: bool,
    pub discovered_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DockerAccess {
    Missing,
    Unavailable,
    PermissionDenied,
    Accessible,
    DetectionFailed,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DockerCapabilities {
    pub cli_installed: bool,
    pub daemon_access: DockerAccess,
    pub client_version: Option<String>,
    pub server_version: Option<String>,
    pub compose_version: Option<String>,
    pub access_without_sudo: bool,
    pub version_detection_failed: bool,
    pub discovered_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ServerMetrics {
    pub cpu_percent: Option<f64>,
    pub memory_percent: Option<f64>,
    pub disk_percent: Option<f64>,
    pub uptime_seconds: f64,
    pub docker_available: bool,
    pub sampled_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SshMetadata {
    pub key_path: Option<String>,
    pub passphrase_saved: bool,
    pub trusted_fingerprint: Option<String>,
    pub trusted_algorithm: Option<String>,
    pub last_test: Option<ConnectionTestResult>,
    pub last_success_at: Option<String>,
    pub info: Option<ServerInfo>,
    pub docker: Option<DockerCapabilities>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub revision: u64,
    pub key_path: Option<String>,
    pub credential_ref: Option<String>,
    pub trusted_key: Option<Vec<u8>>,
    pub trusted_algorithm: Option<String>,
    pub last_test: Option<ConnectionTestResult>,
    pub last_success_at: Option<String>,
    pub info: Option<ServerInfo>,
    pub docker: Option<DockerCapabilities>,
}
