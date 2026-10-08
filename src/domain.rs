use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, path::Path};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceConfig {
    pub id: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub restart_on_failure: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub config: ServiceConfig,
    pub status: ServiceStatus,
}
impl Service {
    /// Placeholder for a service whose definition cannot be read, so it stays visible.
    pub fn unreadable(id: &str, error: String) -> Self {
        Self {
            config: ServiceConfig {
                id: id.into(),
                executable: String::new(),
                arguments: vec![],
                working_directory: None,
                environment: BTreeMap::new(),
                autostart: false,
                restart_on_failure: false,
            },
            status: ServiceStatus {
                state: ServiceState::Error,
                pid: None,
                uptime_seconds: None,
                restart_count: None,
                last_exit_code: None,
                error: Some(error),
            },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub state: ServiceState,
    pub pid: Option<u32>,
    pub uptime_seconds: Option<u64>,
    pub restart_count: Option<u64>,
    pub last_exit_code: Option<i32>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceState {
    Running,
    Stopped,
    Starting,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSnapshot {
    pub stdout: String,
    pub stderr: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Validation,
    NotFound,
    Conflict,
    Io,
    Command,
}
#[derive(Debug, Clone)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}
impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for AppError {}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, e.to_string())
    }
}
pub trait ServiceManager: Send + Sync {
    fn list(&self) -> Result<Vec<Service>, AppError>;
    fn get(&self, id: &str) -> Result<Service, AppError>;
    fn create(&self, config: ServiceConfig) -> Result<Service, AppError>;
    fn update(&self, id: &str, config: ServiceConfig) -> Result<Service, AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
    fn action(&self, id: &str, action: ServiceAction) -> Result<Service, AppError>;
    fn logs(&self, id: &str) -> Result<LogSnapshot, AppError>;
}
pub fn validate_id(id: &str) -> Result<(), AppError> {
    if id.is_empty()
        || id.len() > 100
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(AppError::new(
            ErrorKind::Validation,
            "Service ID must contain 1–100 ASCII letters, digits, hyphens or underscores, starting with a letter or digit",
        ));
    }
    Ok(())
}
impl ServiceConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        validate_id(&self.id)?;
        if !Path::new(&self.executable).is_absolute() || self.executable.contains('\0') {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Executable must be an absolute path",
            ));
        }
        if self
            .working_directory
            .as_ref()
            .is_some_and(|p| !Path::new(p).is_absolute() || p.contains('\0'))
        {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Working directory must be an absolute path",
            ));
        }
        if self.arguments.iter().any(|a| a.contains('\0'))
            || self
                .environment
                .iter()
                .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
        {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Invalid argument or environment variable",
            ));
        }
        if self.restart_on_failure && !self.autostart {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Restart on failure requires automatic start (launchd SuccessfulExit policy)",
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_traversal_and_unicode() {
        for id in ["", "../foo", "a/b", "é", ".foo", "a.plist"] {
            assert!(validate_id(id).is_err(), "{id}");
        }
        assert!(validate_id("llama-swap_2").is_ok());
    }
    #[test]
    fn rejects_keepalive_without_autostart() {
        let c = ServiceConfig {
            id: "test".into(),
            executable: "/bin/echo".into(),
            arguments: vec![],
            working_directory: None,
            environment: BTreeMap::new(),
            autostart: false,
            restart_on_failure: true,
        };
        assert!(c.validate().is_err());
    }
}
