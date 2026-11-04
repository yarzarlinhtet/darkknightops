use super::{failed, Command, Operation};
use crate::{
    domain::{provisioning::*, AppError},
    infrastructure::{
        discovery,
        ssh::{CancellationToken, RemoteCommand, RemoteSession},
    },
};
use std::time::{Duration, Instant};

pub fn inspect(session: &mut dyn RemoteSession) -> Result<Preflight, AppError> {
    let token = CancellationToken::default();
    let os = session.execute(
        RemoteCommand::OsRelease,
        &token,
        Instant::now() + Duration::from_secs(90),
    )?;
    if os.exit_code != 0 {
        return Err(failed("Cannot read /etc/os-release."));
    }
    let result = session.execute(
        RemoteCommand::Provisioning(Command {
            operation: Operation::Preflight,
            run: uuid::Uuid::nil(),
            step: 0,
            root: false,
        }),
        &token,
        Instant::now() + Duration::from_secs(90),
    )?;
    if result.exit_code != 0 {
        return Err(failed("Read-only provisioning checks failed."));
    }
    parse(&os.stdout, &result.stdout)
}
pub fn parse(os: &str, output: &str) -> Result<Preflight, AppError> {
    let values: Vec<_> = output.lines().filter_map(|l| l.split_once('=')).collect();
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or("")
    };
    if get("username").is_empty() || get("uid").parse::<u32>().is_err() {
        return Err(failed("Malformed system identity response."));
    }
    let yes = |key| get(key) == "yes";
    let engine = yes("engine");
    let compose = get("compose").trim_start_matches('v');
    let compose_ok = compose
        .split('.')
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .is_some_and(|v| v >= 2);
    let healthy = matches!(get("access"), "direct" | "administrative");
    let repository_files: Vec<_> = values
        .iter()
        .filter(|(k, _)| *k == "repository_file")
        .collect();
    let repository_lines: Vec<_> = values
        .iter()
        .filter(|(k, _)| *k == "repository_line")
        .map(|(_, v)| *v)
        .collect();
    let expected = format!("Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: noble\nComponents: stable\nArchitectures: {}\nSigned-By: /etc/apt/keyrings/docker.asc", get("architecture"));
    let valid = get("key_count") == "1"
        && repository_files.len() == 1
        && repository_files[0].1 == "/etc/apt/sources.list.d/docker.sources"
        && repository_lines.join("\n").trim() == expected
        && get("key_fingerprint") == "9DC858229FC7DD38854AE2D88D81803C0EBFCD88";
    let available = values
        .iter()
        .filter(|(k, _)| *k == "disk")
        .map(|(_, v)| {
            v.parse::<u64>()
                .ok()
                .and_then(|v| v.checked_mul(1024))
                .unwrap_or(0)
        })
        .min()
        .unwrap_or(0);
    let facts = ProvisioningFacts {
        os_id: discovery::os_release(os, "ID").unwrap_or_default(),
        version_id: discovery::os_release(os, "VERSION_ID").unwrap_or_default(),
        architecture: get("architecture").into(),
        systemd: yes("systemd"),
        apt: yes("apt"),
        administrative_access: matches!(get("privilege"), "root" | "passwordless"),
        repository_reachable: yes("network") && yes("dns") && yes("ubuntu_network"),
        available_bytes: available,
        conflicting_packages: yes("conflicts") && !(healthy && compose_ok),
        custom_repository: !valid
            && (!repository_files.is_empty() || yes("reserved_source") || yes("reserved_key")),
        docker_state_known: get("audit").trim() == "0"
            && get("active_units").trim() == "0"
            && (!yes("cli") || engine || yes("known_packages") || healthy),
        engine_installed: engine || healthy,
        compose_installed: compose_ok,
        additional_packages_required: engine && !yes("buildx"),
        service_running: get("running") == "active",
        service_enabled: get("enabled") == "enabled",
        repository_valid: valid,
    };
    let mut checks = build_plan(&facts);
    if get("service") == "masked" || get("enabled") == "masked" {
        checks.push(PlanStep {
            key: "service_masked".into(),
            status: StepStatus::Blocked,
            description: "Docker service is masked; resolve manually.".into(),
        });
    }
    let clean = |v: &str| discovery::clean(v).chars().take(160).collect::<String>();
    Ok(Preflight {
        checks,
        facts,
        username: clean(get("username")),
        privilege: clean(get("privilege")),
        codename: discovery::os_release(os, "VERSION_CODENAME").unwrap_or_default(),
        kernel: clean(get("kernel")),
        docker_source: clean(get("source")),
        client_version: clean(get("client_version")),
        server_version: clean(get("server_version")),
        compose_version: clean(get("compose")),
        docker_access: clean(get("access")),
        docker_group: get("group") == "docker",
        pending_login: get("group") != "docker" && get("account_group") == "docker",
        workloads: get("workloads").trim().parse().ok(),
        checked_at: discovery::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const OS: &str = "ID=ubuntu\nVERSION_ID=\"24.04\"\nVERSION_CODENAME=noble\n";
    fn output() -> String {
        "username=tester\nuid=1000\narchitecture=amd64\nsystemd=yes\napt=yes\nprivilege=passwordless\ndns=yes\nnetwork=yes\nubuntu_network=yes\ndisk=4000000\naudit=0\nactive_units=0\naccess=unavailable\n".into()
    }
    #[test]
    fn fresh_server_is_distinct_from_inaccessible_installation() {
        let missing = parse(OS, &output()).unwrap();
        assert!(missing.facts.docker_state_known);
        assert!(!missing.facts.engine_installed);
        let cli = parse(OS, &(output() + "cli=yes\n")).unwrap();
        assert!(!cli.facts.docker_state_known);
        let engine = parse(OS, &(output() + "cli=yes\nengine=yes\nrunning=active\n")).unwrap();
        assert!(engine.facts.engine_installed);
        assert_eq!(engine.docker_access, "unavailable");
    }
    #[test]
    fn compose_v1_and_pending_group_are_not_success() {
        let facts = parse(OS, &(output() + "compose=1.29.2\naccount_group=docker\n")).unwrap();
        assert!(!facts.facts.compose_installed);
        assert!(facts.pending_login);
        let facts = parse(OS, &(output() + "compose=2.40.0\n")).unwrap();
        assert!(facts.facts.compose_installed);
    }
    #[test]
    fn untrusted_os_values_are_never_evaluated() {
        let facts = parse("ID=$(touch /tmp/unsafe)\nVERSION_ID=24.04", &output()).unwrap();
        assert!(!compatible(&facts.facts));
        assert!(parse(OS, "uid=not-a-number").is_err());
    }
    #[test]
    fn package_activity_and_audit_fail_closed() {
        for extra in ["audit=20\n", "active_units=1\n"] {
            let base = output()
                .replace("audit=0\n", "")
                .replace("active_units=0\n", "");
            assert!(!parse(OS, &(base + extra)).unwrap().facts.docker_state_known);
        }
    }
    #[test]
    fn smallest_filesystem_and_custom_sources_block_installation() {
        let facts = parse(OS, &(output() + "disk=1\nreserved_source=yes\n")).unwrap();
        assert_eq!(facts.facts.available_bytes, 1024);
        assert!(facts.facts.custom_repository);
        assert!(facts.checks.iter().any(|s| s.status == StepStatus::Blocked));
    }
}
