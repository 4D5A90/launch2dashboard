use crate::domain::{AppError, ErrorKind};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub(super) struct CommandResult {
    pub(super) success: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
}
pub(super) trait Executor: Send + Sync {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandResult, AppError>;
}
pub(super) struct ProcessExecutor;
impl Executor for ProcessExecutor {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandResult, AppError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // Drain both pipes concurrently to avoid blocking when launchctl prints diagnostics.
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out = thread::spawn(move || {
            let mut b = Vec::new();
            stdout.take(1024 * 1024).read_to_end(&mut b).map(|_| b)
        });
        let err = thread::spawn(move || {
            let mut b = Vec::new();
            stderr.take(1024 * 1024).read_to_end(&mut b).map(|_| b)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::new(
                    ErrorKind::Command,
                    format!("{program} timed out after 10 seconds"),
                ));
            }
            thread::sleep(Duration::from_millis(20));
        };
        let stdout = out
            .join()
            .map_err(|_| AppError::new(ErrorKind::Command, "stdout reader failed"))??;
        let stderr = err
            .join()
            .map_err(|_| AppError::new(ErrorKind::Command, "stderr reader failed"))??;
        Ok(CommandResult {
            success: status.success(),
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }
}
