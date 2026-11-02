//! No production SSH adapter is registered in Milestone 1.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);
impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Verification is mandatory; there is intentionally no insecure/ignore option.
#[derive(Debug, Clone)]
pub enum HostKeyPolicy {
    KnownFingerprint(String),
    RequireExplicitTrust,
}
#[derive(Debug, Clone)]
pub struct ExecuteRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub host_key_policy: HostKeyPolicy,
    /// Individual arguments, never interpolated shell text.
    pub arguments: Vec<String>,
    pub timeout: Duration,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuteOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshError {
    Unavailable,
    Cancelled,
    Timeout,
    HostKeyUntrusted,
}
pub trait SshTransport: Send + Sync {
    fn execute(
        &self,
        request: &ExecuteRequest,
        cancellation: &CancellationToken,
    ) -> Result<ExecuteOutput, SshError>;
}

/// Injectable test double. Never used by the desktop application's command handlers.
pub struct MockSshTransport {
    pub response: Result<ExecuteOutput, SshError>,
}
impl Default for MockSshTransport {
    fn default() -> Self {
        Self {
            response: Err(SshError::Unavailable),
        }
    }
}
impl SshTransport for MockSshTransport {
    fn execute(
        &self,
        request: &ExecuteRequest,
        cancellation: &CancellationToken,
    ) -> Result<ExecuteOutput, SshError> {
        if cancellation.is_cancelled() {
            return Err(SshError::Cancelled);
        }
        if request.timeout.is_zero() {
            return Err(SshError::Timeout);
        }
        match &request.host_key_policy {
            HostKeyPolicy::RequireExplicitTrust => return Err(SshError::HostKeyUntrusted),
            HostKeyPolicy::KnownFingerprint(value) if value.is_empty() => {
                return Err(SshError::HostKeyUntrusted)
            }
            _ => {}
        }
        self.response.clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_does_not_imply_a_connection_and_respects_cancellation_and_trust() {
        let mock = MockSshTransport::default();
        let mut request = ExecuteRequest {
            host: "example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            host_key_policy: HostKeyPolicy::RequireExplicitTrust,
            arguments: vec!["uname".into()],
            timeout: Duration::from_secs(5),
        };
        let token = CancellationToken::default();
        assert_eq!(
            mock.execute(&request, &token),
            Err(SshError::HostKeyUntrusted)
        );
        request.host_key_policy = HostKeyPolicy::KnownFingerprint("test-only".into());
        assert_eq!(mock.execute(&request, &token), Err(SshError::Unavailable));
        request.timeout = Duration::ZERO;
        assert_eq!(mock.execute(&request, &token), Err(SshError::Timeout));
        token.cancel();
        assert_eq!(mock.execute(&request, &token), Err(SshError::Cancelled));
    }
}
