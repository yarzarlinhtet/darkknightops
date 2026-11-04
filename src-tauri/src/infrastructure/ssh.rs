//! Fixed read-only commands. No command text is accepted over IPC.
use crate::domain::{AppError, ErrorCode, Server};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    net::{Shutdown, TcpStream, ToSocketAddrs},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn check(&self, deadline: Instant) -> Result<(), AppError> {
        if self.0.load(Ordering::Relaxed) {
            return Err(error(ErrorCode::Cancelled));
        }
        if Instant::now() >= deadline {
            return Err(error(ErrorCode::Timeout));
        }
        Ok(())
    }
}
pub fn error(code: ErrorCode) -> AppError {
    let message = match code {
        ErrorCode::Cancelled => "Operation cancelled.",
        ErrorCode::Timeout => "SSH operation timed out. Check connectivity and retry.",
        ErrorCode::Unreachable => {
            "Cannot reach the SSH server. Check hostname, port, firewall and network."
        }
        ErrorCode::AuthenticationFailed => {
            "SSH key authentication was rejected. Check the username and authorized public key."
        }
        ErrorCode::PassphraseRequired => "An encrypted key requires its correct passphrase.",
        ErrorCode::InvalidPrivateKey => "The selected file is not a supported SSH private key.",
        ErrorCode::OutputLimit => "Remote output exceeded the safety limit.",
        ErrorCode::HostKeyMismatch => {
            "SECURITY: the server host key has changed. Authentication was blocked."
        }
        ErrorCode::HostKeyUntrusted => {
            "Review and explicitly trust the observed host key before authenticating."
        }
        _ => "The SSH operation failed or the connection was closed.",
    };
    AppError::new(code, message)
}
pub fn fingerprint(key: &[u8]) -> String {
    format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(key)))
}
#[derive(Clone)]
pub struct ObservedKey {
    pub bytes: Vec<u8>,
    pub algorithm: String,
}
pub enum ConnectFailure {
    Error(AppError),
    HostKey(ObservedKey, bool),
}
impl From<AppError> for ConnectFailure {
    fn from(value: AppError) -> Self {
        Self::Error(value)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum RemoteCommand {
    Provisioning(crate::infrastructure::provisioning::Command),
    Probe,
    OsRelease,
    Kernel,
    Architecture,
    Hostname,
    Uptime,
    CpuCount,
    MemInfo,
    Disk,
    Load,
    CpuStat,
    DockerCli,
    DockerClient,
    DockerServer,
    DockerCompose,
}
impl RemoteCommand {
    pub fn script(self) -> String {
        if let Self::Provisioning(command) = self {
            return command.script();
        }
        match self {
            Self::Provisioning(_) => unreachable!("handled above"),
            Self::Probe => "printf remotedock",
            Self::OsRelease => "cat /etc/os-release",
            Self::Kernel => "uname -r",
            Self::Architecture => "uname -m",
            Self::Hostname => "hostname",
            Self::Uptime => "cat /proc/uptime",
            Self::CpuCount => "getconf _NPROCESSORS_ONLN",
            Self::MemInfo => "cat /proc/meminfo",
            Self::Disk => "LC_ALL=C df -Pk /",
            Self::Load => "cat /proc/loadavg",
            Self::CpuStat => "cat /proc/stat",
            Self::DockerCli => "command -v docker",
            Self::DockerClient => "LC_ALL=C docker --version",
            Self::DockerServer => {
                "LC_ALL=C docker --host unix:///var/run/docker.sock version --format '{{json .}}'"
            }
            Self::DockerCompose => "LC_ALL=C docker compose version --short",
        }
        .into()
    }
}
#[derive(Clone, Debug)]
pub struct ExecuteOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
pub trait RemoteSession: Send {
    fn execute(
        &mut self,
        command: RemoteCommand,
        cancellation: &CancellationToken,
        deadline: Instant,
    ) -> Result<ExecuteOutput, AppError>;
}
pub trait SshTransport: Send + Sync {
    fn connect(
        &self,
        server: &Server,
        key_path: &Path,
        passphrase: Option<&str>,
        trusted_key: Option<&[u8]>,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn RemoteSession>, ConnectFailure>;
}
pub struct RealSshTransport;
struct Session {
    ssh: ssh2::Session,
    socket: TcpStream,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.ssh.disconnect(None, "Closing", None);
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}
fn retry<T>(
    token: &CancellationToken,
    deadline: Instant,
    mut action: impl FnMut() -> Result<T, ssh2::Error>,
) -> Result<T, AppError> {
    loop {
        token.check(deadline)?;
        match action() {
            Ok(value) => return Ok(value),
            Err(e) if e.code() == ssh2::ErrorCode::Session(-37) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => return Err(error(ErrorCode::Internal)),
        }
    }
}
// A stuck OS resolver must not create unbounded background threads.
static RESOLVERS: AtomicUsize = AtomicUsize::new(0);
fn resolve(
    server: &Server,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<Vec<std::net::SocketAddr>, AppError> {
    if RESOLVERS
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            (n < 4).then_some(n + 1)
        })
        .is_err()
    {
        return Err(AppError::new(
            ErrorCode::Busy,
            "DNS resolution is busy. Retry later.",
        ));
    }
    let target = (server.host.clone(), server.port);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let value = target
            .to_socket_addrs()
            .map(|v| v.take(16).collect::<Vec<_>>());
        let _ = tx.send(value);
        RESOLVERS.fetch_sub(1, Ordering::SeqCst);
    });
    loop {
        token.check(deadline)?;
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Ok(addresses)) => return Ok(addresses),
            Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(error(ErrorCode::Unreachable))
            }
            Err(_) => {}
        }
    }
}
impl SshTransport for RealSshTransport {
    fn connect(
        &self,
        server: &Server,
        key_path: &Path,
        passphrase: Option<&str>,
        trusted_key: Option<&[u8]>,
        token: &CancellationToken,
    ) -> Result<Box<dyn RemoteSession>, ConnectFailure> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let addresses = resolve(server, token, deadline)?;
        let mut socket = None;
        let mut timed_out = false;
        for address in addresses {
            token.check(deadline)?;
            match TcpStream::connect_timeout(
                &address,
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(3)),
            ) {
                Ok(value) => {
                    socket = Some(value);
                    break;
                }
                Err(e) => {
                    timed_out |= e.kind() == std::io::ErrorKind::TimedOut;
                    continue;
                }
            }
        }
        token.check(deadline)?;
        let socket = socket.ok_or_else(|| {
            error(if timed_out {
                ErrorCode::Timeout
            } else {
                ErrorCode::Unreachable
            })
        })?;
        socket
            .set_nonblocking(true)
            .map_err(|_| error(ErrorCode::Internal))?;
        let ssh = ssh2::Session::new().map_err(|_| error(ErrorCode::Internal))?;
        let mut session = Session { ssh, socket };
        session.ssh.set_tcp_stream(
            session
                .socket
                .try_clone()
                .map_err(|_| error(ErrorCode::Internal))?,
        );
        session.ssh.set_blocking(false);
        session.ssh.method_pref(ssh2::MethodType::HostKey, "ssh-ed25519,rsa-sha2-512,rsa-sha2-256,ecdsa-sha2-nistp256,ecdsa-sha2-nistp384,ecdsa-sha2-nistp521")
            .map_err(|_| error(ErrorCode::Internal))?;
        retry(token, deadline, || session.ssh.handshake())?;
        let (key, _) = session
            .ssh
            .host_key()
            .ok_or_else(|| error(ErrorCode::Internal))?;
        let algorithm = if key.len() >= 4 {
            let length = u32::from_be_bytes([key[0], key[1], key[2], key[3]]) as usize;
            key.get(4..4usize.saturating_add(length))
                .and_then(|v| std::str::from_utf8(v).ok())
                .unwrap_or("unknown")
                .to_owned()
        } else {
            "unknown".into()
        };
        if trusted_key != Some(key) {
            return Err(ConnectFailure::HostKey(
                ObservedKey {
                    bytes: key.to_vec(),
                    algorithm,
                },
                trusted_key.is_some(),
            ));
        }
        // Host identity has been checked before key loading or authentication.
        let file =
            std::fs::File::open(key_path).map_err(|_| error(ErrorCode::KeyFileUnavailable))?;
        let mut bytes = Zeroizing::new(String::new());
        file.take(1024 * 1024 + 1)
            .read_to_string(&mut bytes)
            .map_err(|_| error(ErrorCode::InvalidPrivateKey))?;
        if bytes.len() > 1024 * 1024 {
            return Err(error(ErrorCode::InvalidPrivateKey).into());
        }
        let key = ssh_key::PrivateKey::from_openssh(bytes.as_str())
            .map_err(|_| error(ErrorCode::InvalidPrivateKey))?;
        let key = if key.is_encrypted() {
            key.decrypt(passphrase.ok_or_else(|| error(ErrorCode::PassphraseRequired))?)
                .map_err(|_| error(ErrorCode::PassphraseRequired))?
        } else {
            key
        };
        let encoded = key
            .to_openssh(ssh_key::LineEnding::LF)
            .map_err(|_| error(ErrorCode::InvalidPrivateKey))?;
        retry(token, deadline, || {
            session
                .ssh
                .userauth_pubkey_memory(&server.username, None, encoded.as_str(), None)
        })
        .map_err(|e| {
            if matches!(e.code, ErrorCode::Timeout | ErrorCode::Cancelled) {
                e
            } else {
                error(ErrorCode::AuthenticationFailed)
            }
        })?;
        if !session.ssh.authenticated() {
            return Err(error(ErrorCode::AuthenticationFailed).into());
        }
        Ok(Box::new(session))
    }
}
impl RemoteSession for Session {
    fn execute(
        &mut self,
        command: RemoteCommand,
        token: &CancellationToken,
        outer_deadline: Instant,
    ) -> Result<ExecuteOutput, AppError> {
        let limit = if matches!(command, RemoteCommand::Provisioning(_)) {
            90
        } else {
            10
        };
        let deadline = outer_deadline.min(Instant::now() + Duration::from_secs(limit));
        let mut channel = retry(token, deadline, || self.ssh.channel_session())?;
        retry(token, deadline, || channel.exec(&command.script()))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        loop {
            token.check(deadline)?;
            let mut progress = false;
            for (stream, output) in [(0, &mut stdout), (1, &mut stderr)] {
                let mut buffer = [0u8; 8192];
                match channel.stream(stream).read(&mut buffer) {
                    Ok(n) if n > 0 => {
                        output.extend_from_slice(&buffer[..n]);
                        progress = true;
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => return Err(error(ErrorCode::Internal)),
                }
            }
            if stdout.len() + stderr.len() > 1024 * 1024 {
                return Err(error(ErrorCode::OutputLimit));
            }
            if channel.eof() && !progress {
                break;
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        retry(token, deadline, || channel.wait_close())?;
        if channel
            .exit_signal()
            .map_err(|_| error(ErrorCode::Internal))?
            .exit_signal
            .is_some()
        {
            return Err(error(ErrorCode::Internal));
        }
        let exit_code = channel
            .exit_status()
            .map_err(|_| error(ErrorCode::Internal))?;
        Ok(ExecuteOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprints_are_openssh_sha256() {
        assert_eq!(
            fingerprint(b"abc"),
            "SHA256:ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0"
        );
    }
    #[test]
    fn cancelled_and_expired_operations_fail() {
        let token = CancellationToken::default();
        assert_eq!(
            token.check(Instant::now()).unwrap_err().code,
            ErrorCode::Timeout
        );
        token.cancel();
        assert_eq!(
            token
                .check(Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
    }
}
