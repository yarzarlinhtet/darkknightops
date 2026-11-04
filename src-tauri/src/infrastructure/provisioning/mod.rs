//! Reviewed, closed command vocabulary. No script or path comes from IPC.
use crate::domain::{AppError, ErrorCode};
#[derive(Debug, Clone, Copy)]
pub enum Operation {
    Preflight,
    FixtureProbe,
    Bootstrap,
    Repository,
    Packages,
    Enable,
    Start,
    Grant,
    Inspect,
    Logs,
}
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub operation: Operation,
    pub run: uuid::Uuid,
    pub step: u8,
    pub root: bool,
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
impl Command {
    pub fn unit(&self) -> String {
        format!("remotedock-provision-{}-{}", self.run.simple(), self.step)
    }
    pub fn script(&self) -> String {
        let privilege = if self.root { "" } else { "sudo -n " };
        if matches!(self.operation, Operation::FixtureProbe) {
            return "cat /etc/remotedock-disposable-provisioning-fixture".into();
        }
        if matches!(self.operation, Operation::Preflight) {
            return include_str!("preflight.sh").into();
        }
        if matches!(self.operation, Operation::Logs) {
            return format!("LC_ALL=C {privilege}/usr/bin/journalctl --no-pager --output=cat -n 100 --unit={}.service", self.unit());
        }
        if matches!(self.operation, Operation::Inspect) {
            return format!("LC_ALL=C {privilege}/usr/bin/systemctl show {}.service --property=LoadState,ActiveState,SubState,Result,ExecMainStatus", self.unit());
        }
        let script = match self.operation {
            Operation::Bootstrap => include_str!("bootstrap.sh"),
            Operation::Repository => include_str!("repository.sh"),
            Operation::Packages => include_str!("packages.sh"),
            Operation::Enable => "set -eu; systemctl is-enabled --quiet docker.service || systemctl enable docker.service",
            Operation::Start => "set -eu; systemctl is-active --quiet docker.service || systemctl start docker.service",
            // Username comes from SSH session identity, never interpolated into syntax.
            Operation::Grant => "set -eu; test -n \"$1\"; getent group docker >/dev/null; /usr/sbin/usermod -aG docker -- \"$1\"; id -nG \"$1\" | tr ' ' '\\n' | grep -x docker",
            _ => return String::new(),
        };
        // Resolve SSH identity before privilege escalation; always pass it as one argument.
        format!("{privilege}/usr/bin/systemd-run --quiet --unit={} --property=Type=oneshot --property=RemainAfterExit=yes --property=StandardOutput=journal --property=StandardError=journal /bin/sh -c {} remotedock \"$(id -un)\"", self.unit(), quote(script))
    }
}
pub fn failed(message: &str) -> AppError {
    AppError::new(ErrorCode::ProvisioningFailed, message)
}

pub mod persistence;
pub mod preflight;

/// Export only known package phase/name pairs. URLs, versions, script output,
/// terminal controls and arbitrary remote messages never cross the log boundary.
pub fn sanitized_log(output: &str) -> String {
    let packages = [
        "docker-ce",
        "docker-ce-cli",
        "containerd.io",
        "docker-buildx-plugin",
        "docker-compose-plugin",
        "ca-certificates",
        "curl",
        "gnupg",
    ];
    output
        .lines()
        .filter_map(|line| {
            let (phase, rest) = if let Some(rest) = line.strip_prefix("Setting up ") {
                ("Setting up", rest)
            } else {
                ("Unpacking", line.strip_prefix("Unpacking ")?)
            };
            let package = rest.split_whitespace().next()?;
            packages
                .contains(&package)
                .then(|| format!("{phase} {package}"))
        })
        .take(100)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privileged_operations_are_detached_and_closed() {
        for operation in [
            Operation::Bootstrap,
            Operation::Repository,
            Operation::Packages,
            Operation::Enable,
            Operation::Start,
            Operation::Grant,
        ] {
            let script = Command {
                operation,
                run: uuid::Uuid::nil(),
                step: 3,
                root: false,
            }
            .script();
            assert!(script.starts_with("sudo -n /usr/bin/systemd-run"));
            assert!(script.contains("--property=RemainAfterExit=yes"));
            assert!(!script.contains("curl |"));
        }
        assert!(!Command {
            operation: Operation::Inspect,
            run: uuid::Uuid::nil(),
            step: 0,
            root: true
        }
        .script()
        .contains("sudo"));
    }
    #[test]
    fn logs_export_only_known_package_phases() {
        let output="https://user:secret@example.com\nSetting up docker-ce (29.0 secret)\nUnpacking curl (secret)\nSetting up malicious-secret\nTOKEN=secret\n\u{1b}[31msecret";
        assert_eq!(
            sanitized_log(output),
            "Setting up docker-ce\nUnpacking curl"
        );
    }
    #[test]
    fn scripts_parse_with_posix_shell() {
        for script in [
            include_str!("preflight.sh"),
            include_str!("bootstrap.sh"),
            include_str!("repository.sh"),
            include_str!("packages.sh"),
        ] {
            use std::io::Write;
            let mut child = std::process::Command::new("/bin/sh")
                .arg("-n")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
        }
    }
}

#[cfg(test)]
mod transaction_tests {
    use std::{io::Write, os::unix::fs::PermissionsExt};
    fn run(simulation: &str) -> (bool, String) {
        let dir = tempfile::tempdir().unwrap();
        let apt = dir.path().join("apt-get");
        std::fs::write(&apt,"#!/bin/sh\ncase \" $* \" in *' --simulate '*) printf '%s\\n' \"$SIMULATION\";; *' install '*) printf '%s\\n' \"$*\" >> \"$CALLS\";; esac\n").unwrap();
        std::fs::set_permissions(&apt, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dpkg = dir.path().join("dpkg-query");
        std::fs::write(&dpkg, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&dpkg, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cache = dir.path().join("apt-cache");
        std::fs::write(&cache,"#!/bin/sh\ncase \"$1\" in policy) echo '  Candidate: 1.0';; madison) echo \"$2 | 1.0 | https://download.docker.com/linux/ubuntu noble/stable amd64 Packages\";; esac\n").unwrap();
        std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700)).unwrap();
        let calls = dir.path().join("calls");
        let mut child = std::process::Command::new("/bin/sh")
            .env("PATH", format!("{}:/usr/bin:/bin", dir.path().display()))
            .env("SIMULATION", simulation)
            .env("CALLS", &calls)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(include_str!("packages.sh").as_bytes())
            .unwrap();
        let success = child.wait().unwrap().success();
        (success, std::fs::read_to_string(calls).unwrap_or_default())
    }
    #[test]
    fn missing_package_transaction_pins_all_packages_and_runs() {
        let (success,calls)=run("Inst docker-ce (1.0 Docker CE:stable [amd64])\nInst docker-compose-plugin (1.0 Docker CE:stable [amd64])");
        assert!(success);
        assert!(calls.contains("docker-ce=1.0 docker-compose-plugin=1.0"));
    }
    #[test]
    fn removal_and_upgrade_transactions_never_install() {
        for simulation in [
            "Remv containerd [1.0]",
            "Inst containerd.io [0.9] (1.0 Docker CE:stable [amd64])",
        ] {
            let (success, calls) = run(simulation);
            assert!(!success);
            assert!(calls.is_empty());
        }
    }
}
