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
pub const MAX_ICON_BYTES: usize = 5 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconFormat {
    Png,
    Svg,
}
impl IconFormat {
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Svg => "image/svg+xml",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }
}
/// A service icon whose format was checked from its content, never from a name or header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    format: IconFormat,
    bytes: Vec<u8>,
}
impl Icon {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, AppError> {
        if bytes.len() > MAX_ICON_BYTES {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Icon must be 5 MB or smaller",
            ));
        }
        let format = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            IconFormat::Png
        } else if std::str::from_utf8(&bytes)
            .is_ok_and(|text| text.to_ascii_lowercase().contains("<svg"))
        {
            IconFormat::Svg
        } else {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Icon must be a PNG or SVG image",
            ));
        };
        Ok(Self { format, bytes })
    }
    pub fn format(&self) -> IconFormat {
        self.format
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub trait IconStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<Icon>, AppError>;
    /// Opaque token that changes whenever the icon is replaced.
    fn version(&self, id: &str) -> Result<Option<String>, AppError>;
    fn put(&self, id: &str, icon: &Icon) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
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
            assert_eq!(
                validate_id(id).unwrap_err().kind,
                ErrorKind::Validation,
                "{id}"
            );
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
        assert_eq!(c.validate().unwrap_err().kind, ErrorKind::Validation);
    }
    #[test]
    fn icon_format_comes_from_content() {
        let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
        assert_eq!(Icon::parse(png).unwrap().format(), IconFormat::Png);
        for svg in [
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            "\u{feff}<?xml version=\"1.0\"?>\n<!-- logo -->\n<SVG viewBox=\"0 0 1 1\"></SVG>",
        ] {
            let icon = Icon::parse(svg.as_bytes().to_vec()).unwrap();
            assert_eq!(icon.format(), IconFormat::Svg);
            assert_eq!(icon.bytes(), svg.as_bytes());
        }
    }
    #[test]
    fn icon_rejects_other_content_and_oversize() {
        let mut huge = b"\x89PNG\r\n\x1a\n".to_vec();
        huge.resize(MAX_ICON_BYTES + 1, 0);
        for bytes in [
            vec![],
            b"GIF89a".to_vec(),
            b"<html><body>not an icon</body></html>".to_vec(),
            vec![0xff, 0xfe, b'<', b's', b'v', b'g'],
            huge,
        ] {
            assert_eq!(Icon::parse(bytes).unwrap_err().kind, ErrorKind::Validation);
        }
        let mut max = b"\x89PNG\r\n\x1a\n".to_vec();
        max.resize(MAX_ICON_BYTES, 0);
        assert!(Icon::parse(max).is_ok());
    }
}
