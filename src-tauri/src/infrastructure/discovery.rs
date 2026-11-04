//! Pure, read-only Linux resource counter parsing.
#[derive(Debug, PartialEq, Eq)]
pub struct CpuCounters {
    pub total: u64,
    pub idle: u64,
}
impl CpuCounters {
    pub fn parse(stat: &str) -> Option<Self> {
        let mut fields = stat.lines().next()?.split_whitespace();
        if fields.next()? != "cpu" {
            return None;
        }
        // guest and guest_nice are already included in user and nice.
        let values = fields
            .take(8)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if values.len() < 4 {
            return None;
        }
        let total = values
            .iter()
            .try_fold(0u64, |sum, value| sum.checked_add(*value))?;
        let idle = values[3].checked_add(values.get(4).copied().unwrap_or(0))?;
        Some(Self { total, idle })
    }
    pub fn utilization_since(&self, previous: &Self) -> Option<f64> {
        let total = self.total.checked_sub(previous.total)?;
        let idle = self.idle.checked_sub(previous.idle)?;
        if total == 0 || idle > total {
            return None;
        }
        Some(100.0 * (total - idle) as f64 / total as f64)
    }
}

pub fn memory_utilization(meminfo: &str) -> Option<f64> {
    let value = |name: &str| -> Option<u64> {
        let line = meminfo.lines().find(|line| line.starts_with(name))?;
        let mut fields = line.split_whitespace();
        if fields.next()? != name {
            return None;
        }
        let value = fields.next()?.parse().ok()?;
        if fields.next()? != "kB" || fields.next().is_some() {
            return None;
        }
        Some(value)
    };
    let total = value("MemTotal:")?;
    let available = value("MemAvailable:")?;
    if total == 0 || available > total {
        return None;
    }
    Some(100.0 * (total - available) as f64 / total as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_excludes_guest_and_samples_counters() {
        let before = CpuCounters::parse("cpu 100 0 0 100 0 0 0 0 80 0").unwrap();
        let after = CpuCounters::parse("cpu 150 0 0 150 0 0 0 0 100 0").unwrap();
        assert_eq!(after.utilization_since(&before), Some(50.0));
        assert_eq!(before.utilization_since(&after), None);
        assert_eq!(after.utilization_since(&after), None);
    }
    #[test]
    fn rejects_invalid_counters() {
        for input in [
            "",
            "cpu0 1 2 3 4",
            "cpu 1 2",
            "cpu a 2 3 4",
            "cpu 18446744073709551615 1 0 0",
        ] {
            assert_eq!(CpuCounters::parse(input), None);
        }
    }
    #[test]
    fn memory_uses_available_including_reclaimable_memory() {
        assert_eq!(
            memory_utilization("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 600 kB"),
            Some(40.0)
        );
        for input in [
            "",
            "MemTotal: 0 kB\nMemAvailable: 0 kB",
            "MemTotal: 10 kB\nMemAvailable: 11 kB",
            "MemTotal: 10 MB\nMemAvailable: 1 kB",
        ] {
            assert_eq!(memory_utilization(input), None);
        }
    }
}

use crate::domain::{connection::*, AppError, ErrorCode};
use crate::infrastructure::ssh::{CancellationToken, ExecuteOutput, RemoteCommand, RemoteSession};
use std::time::{Duration, Instant};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn invalid() -> AppError {
    AppError::new(
        ErrorCode::DiscoveryFailed,
        "The server returned incomplete or unsupported discovery data.",
    )
}
pub fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}
fn run(
    session: &mut dyn RemoteSession,
    command: RemoteCommand,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<String, AppError> {
    let output = session.execute(command, token, deadline)?;
    if output.exit_code != 0 {
        return Err(invalid());
    }
    Ok(output.stdout)
}
pub fn os_release(value: &str, name: &str) -> Option<String> {
    value
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| clean(value.trim().trim_matches('"').trim_matches('\'')))
}
fn number(value: &str) -> Result<f64, AppError> {
    let v: f64 = value.parse().map_err(|_| invalid())?;
    if !v.is_finite() || v < 0.0 {
        return Err(invalid());
    }
    Ok(v)
}
fn memory(value: &str, key: &str) -> Result<f64, AppError> {
    let line = value
        .lines()
        .find(|l| l.starts_with(key))
        .ok_or_else(invalid)?;
    let mut parts = line.split_whitespace();
    if parts.next() != Some(key) {
        return Err(invalid());
    }
    let value = number(parts.next().ok_or_else(invalid)?)?;
    if parts.next() != Some("kB") {
        return Err(invalid());
    }
    Ok(value * 1024.0)
}
pub fn disk(value: &str) -> Result<(f64, f64), AppError> {
    let parts: Vec<_> = value
        .lines()
        .nth(1)
        .ok_or_else(invalid)?
        .split_whitespace()
        .collect();
    if parts.len() < 6 {
        return Err(invalid());
    }
    let total = number(parts[1])? * 1024.0;
    let available = number(parts[3])? * 1024.0;
    if total <= 0.0 || available > total {
        return Err(invalid());
    }
    Ok((total, available))
}
pub fn info(
    session: &mut dyn RemoteSession,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<ServerInfo, AppError> {
    let os = run(session, RemoteCommand::OsRelease, token, deadline)?;
    let kernel = clean(run(session, RemoteCommand::Kernel, token, deadline)?.trim());
    let architecture = clean(run(session, RemoteCommand::Architecture, token, deadline)?.trim());
    let hostname = clean(run(session, RemoteCommand::Hostname, token, deadline)?.trim());
    let uptime = run(session, RemoteCommand::Uptime, token, deadline)?;
    let uptime_seconds = number(uptime.split_whitespace().next().ok_or_else(invalid)?)?;
    let logical_cpus = run(session, RemoteCommand::CpuCount, token, deadline)?
        .trim()
        .parse::<u32>()
        .map_err(|_| invalid())?;
    if logical_cpus == 0 {
        return Err(invalid());
    }
    let mem = run(session, RemoteCommand::MemInfo, token, deadline)?;
    let memory_total_bytes = memory(&mem, "MemTotal:")?;
    let memory_available_bytes = memory(&mem, "MemAvailable:")?;
    if memory_available_bytes > memory_total_bytes || memory_total_bytes == 0.0 {
        return Err(invalid());
    }
    let (disk_total_bytes, disk_available_bytes) =
        disk(&run(session, RemoteCommand::Disk, token, deadline)?)?;
    let load_average = run(session, RemoteCommand::Load, token, deadline)?
        .split_whitespace()
        .take(3)
        .map(number)
        .collect::<Result<Vec<_>, _>>()?;
    if load_average.len() != 3 {
        return Err(invalid());
    }
    Ok(ServerInfo {
        distribution: os_release(&os, "PRETTY_NAME"),
        version: os_release(&os, "VERSION_ID"),
        provisioning_supported: os_release(&os, "ID").as_deref() == Some("ubuntu")
            && os_release(&os, "VERSION_ID").as_deref() == Some("24.04"),
        kernel,
        architecture,
        hostname,
        uptime_seconds,
        logical_cpus,
        memory_total_bytes,
        memory_available_bytes,
        disk_total_bytes,
        disk_available_bytes,
        load_average,
        discovered_at: now(),
    })
}
pub fn parse_docker(
    cli: &ExecuteOutput,
    client: &ExecuteOutput,
    server: &ExecuteOutput,
    compose: &ExecuteOutput,
) -> DockerCapabilities {
    let cli_installed = cli.exit_code == 0 && !cli.stdout.trim().is_empty();
    let json = serde_json::from_str::<serde_json::Value>(&server.stdout).ok();
    let version = |part: &str| {
        json.as_ref()
            .and_then(|v| v.get(part))
            .and_then(|v| v.get("Version"))
            .and_then(|v| v.as_str())
            .map(clean)
    };
    let client_version = version("Client").or_else(|| {
        (client.exit_code == 0)
            .then(|| {
                client
                    .stdout
                    .trim()
                    .strip_prefix("Docker version ")
                    .and_then(|v| v.split(',').next())
                    .map(clean)
            })
            .flatten()
    });
    let server_version = version("Server");
    let daemon_access = if !cli_installed {
        DockerAccess::Missing
    } else if server.exit_code == 0 && server_version.is_some() {
        DockerAccess::Accessible
    } else if server.stderr.to_lowercase().contains("permission denied") {
        DockerAccess::PermissionDenied
    } else if server.exit_code != 0 {
        DockerAccess::Unavailable
    } else {
        DockerAccess::DetectionFailed
    };
    DockerCapabilities {
        cli_installed,
        access_without_sudo: daemon_access == DockerAccess::Accessible,
        version_detection_failed: cli_installed
            && (client_version.is_none() || daemon_access == DockerAccess::DetectionFailed),
        daemon_access,
        client_version,
        server_version,
        compose_version: (compose.exit_code == 0 && !compose.stdout.trim().is_empty())
            .then(|| clean(compose.stdout.trim())),
        discovered_at: now(),
    }
}
pub fn docker(
    session: &mut dyn RemoteSession,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<DockerCapabilities, AppError> {
    let cli = session.execute(RemoteCommand::DockerCli, token, deadline)?;
    let client = session.execute(RemoteCommand::DockerClient, token, deadline)?;
    let server = session.execute(RemoteCommand::DockerServer, token, deadline)?;
    let compose = session.execute(RemoteCommand::DockerCompose, token, deadline)?;
    Ok(parse_docker(&cli, &client, &server, &compose))
}
pub fn metrics(
    session: &mut dyn RemoteSession,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<ServerMetrics, AppError> {
    let before = CpuCounters::parse(&run(session, RemoteCommand::CpuStat, token, deadline)?);
    let sample_at = Instant::now();
    while sample_at.elapsed() < Duration::from_secs(1) {
        token.check(deadline)?;
        std::thread::sleep(Duration::from_millis(20));
    }
    let after = CpuCounters::parse(&run(session, RemoteCommand::CpuStat, token, deadline)?);
    let mem = run(session, RemoteCommand::MemInfo, token, deadline)?;
    let (total, available) = disk(&run(session, RemoteCommand::Disk, token, deadline)?)?;
    let uptime = run(session, RemoteCommand::Uptime, token, deadline)?;
    let daemon = session.execute(RemoteCommand::DockerServer, token, deadline)?;
    Ok(ServerMetrics {
        cpu_percent: before.zip(after).and_then(|(b, a)| a.utilization_since(&b)),
        memory_percent: memory_utilization(&mem),
        disk_percent: Some(100.0 * (total - available) / total),
        uptime_seconds: number(uptime.split_whitespace().next().ok_or_else(invalid)?)?,
        docker_available: daemon.exit_code == 0
            && serde_json::from_str::<serde_json::Value>(&daemon.stdout)
                .ok()
                .and_then(|v| v.get("Server").and_then(|s| s.get("Version")).cloned())
                .is_some(),
        sampled_at: now(),
    })
}
#[cfg(test)]
mod discovery_tests {
    use super::*;
    fn output(text: &str, error: &str, exit_code: i32) -> ExecuteOutput {
        ExecuteOutput {
            stdout: text.into(),
            stderr: error.into(),
            exit_code,
        }
    }
    #[test]
    fn os_is_parsed_as_data() {
        let os = "ID=ubuntu\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu $(touch /tmp/never)\"";
        assert_eq!(os_release(os, "VERSION_ID").as_deref(), Some("24.04"));
        assert!(os_release(os, "PRETTY_NAME").unwrap().contains("$("));
    }
    #[test]
    fn docker_distinguishes_cli_daemon_permissions_and_invalid_output() {
        let ok = output("/usr/bin/docker", "", 0);
        let missing = output("", "", 127);
        let denied = output("", "permission denied while connecting", 1);
        let down = output("", "Cannot connect", 1);
        let versions = output(
            r#"{"Client":{"Version":"27.1"},"Server":{"Version":"27.1"}}"#,
            "",
            0,
        );
        assert_eq!(
            parse_docker(&missing, &missing, &missing, &missing).daemon_access,
            DockerAccess::Missing
        );
        assert_eq!(
            parse_docker(&ok, &ok, &denied, &missing).daemon_access,
            DockerAccess::PermissionDenied
        );
        assert_eq!(
            parse_docker(&ok, &ok, &down, &missing).daemon_access,
            DockerAccess::Unavailable
        );
        assert_eq!(
            parse_docker(&ok, &ok, &ok, &missing).daemon_access,
            DockerAccess::DetectionFailed
        );
        let caps = parse_docker(&ok, &ok, &versions, &missing);
        assert_eq!(caps.daemon_access, DockerAccess::Accessible);
        assert!(caps.compose_version.is_none());
    }
    #[test]
    fn disk_units_and_bad_data() {
        assert_eq!(disk("Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/vda 1000 400 600 40% /").unwrap(),(1024000.0,614400.0));
        assert!(disk("x\ny NaN 0 1 0 /").is_err());
    }
}
