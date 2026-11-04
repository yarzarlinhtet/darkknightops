pub mod connection;
pub mod keys;
pub mod provisioning;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Validation,
    NotFound,
    DatabaseUnavailable,
    DatabaseError,
    MigrationFailed,
    Internal,
    Busy,
    Cancelled,
    Timeout,
    Unreachable,
    AuthenticationFailed,
    PassphraseRequired,
    InvalidPrivateKey,
    KeyFileUnavailable,
    CredentialStoreUnavailable,
    HostKeyUntrusted,
    HostKeyMismatch,
    ObservationExpired,
    OutputLimit,
    DiscoveryFailed,
    ProvisioningFailed,
    ProvisioningUnknown,
    PlanStale,
    ProvisioningBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub field: Option<String>,
}
impl AppError {
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
            field: None,
        }
    }
    pub fn validation(field: &str, message: &str) -> Self {
        Self {
            code: ErrorCode::Validation,
            message: message.into(),
            field: Some(field.into()),
        }
    }
    pub fn not_found() -> Self {
        Self::new(ErrorCode::NotFound, "Server no longer exists.")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ServerInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub notes: Option<String>,
}
impl ServerInput {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 255
            || self.name.chars().any(char::is_control)
        {
            return Err(AppError::validation(
                "name",
                "Enter a name of 1–255 characters without control characters.",
            ));
        }
        if self.port == 0 {
            return Err(AppError::validation(
                "port",
                "Port must be between 1 and 65535.",
            ));
        }
        let ip = self.host.parse::<std::net::IpAddr>().is_ok();
        let dns = !self.host.is_empty()
            && self.host.len() <= 253
            && self.host.split('.').all(|p| {
                !p.is_empty()
                    && p.len() <= 63
                    && !p.starts_with('-')
                    && !p.ends_with('-')
                    && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
        let malformed_ip = self.host.bytes().all(|b| b.is_ascii_digit() || b == b'.') && !ip;
        if (!ip && !dns) || malformed_ip {
            return Err(AppError::validation(
                "host",
                "Enter a hostname or IP address, without a URL or port.",
            ));
        }
        let mut username = self.username.bytes();
        if !matches!(username.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
            || self.username.len() > 64
            || !username.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(AppError::validation(
                "username",
                "Use a username beginning with a letter or underscore, up to 64 characters.",
            ));
        }
        if self.notes.as_ref().is_some_and(|n| {
            n.chars().count() > 4096 || n.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        }) {
            return Err(AppError::validation("notes", "Notes must contain at most 4096 characters and no control characters except tabs and newlines."));
        }
        Ok(())
    }
}

pub fn validate_id(id: &str) -> Result<(), AppError> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::validation("id", "Invalid server identifier."))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn input() -> ServerInput {
        ServerInput {
            name: "Production".into(),
            host: "vps.example.com".into(),
            port: 22,
            username: "ubuntu".into(),
            notes: None,
        }
    }
    #[test]
    fn validates_hostnames_and_ips() {
        for host in ["example.com", "localhost", "192.0.2.1", "2001:db8::1"] {
            let mut value = input();
            value.host = host.into();
            assert!(value.validate().is_ok(), "{host}");
        }
        for host in [
            "",
            "https://example.com",
            "example.com:22",
            "a;whoami",
            "-a.com",
            "a..com",
            "a\n",
            "999.2.3.4",
            "[::1]",
            "a b",
        ] {
            let mut value = input();
            value.host = host.into();
            assert!(value.validate().is_err(), "{host}");
        }
    }
    #[test]
    fn validates_boundaries() {
        let mut value = input();
        value.port = 65535;
        assert!(value.validate().is_ok());
        value.port = 0;
        assert_eq!(value.validate().unwrap_err().field.as_deref(), Some("port"));
        let mut value = input();
        value.name = " ".into();
        assert!(value.validate().is_err());
        value.name = "a".repeat(255);
        assert!(value.validate().is_ok());
        value.name.push('a');
        assert!(value.validate().is_err());
        let mut value = input();
        value.username = "-root".into();
        assert!(value.validate().is_err());
        value.username = "_user-1".into();
        value.notes = Some("n".repeat(4096));
        assert!(value.validate().is_ok());
        value.notes = Some("n".repeat(4097));
        assert!(value.validate().is_err());
        value.notes = Some("secret\0".into());
        assert!(value.validate().is_err());
        assert!(validate_id("../../file").is_err());
    }
}
