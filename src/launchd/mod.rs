mod definition;
mod executor;
mod secure_fs;
mod session;
mod status;
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;

use crate::domain::*;
use executor::{Executor, ProcessExecutor};
use plist::{Dictionary, Value};
use secure_fs::{open_read, safe_directory, safe_regular};
use session::LaunchDomain;
use status::JobStatus;
use std::{fs, path::PathBuf, sync::Arc};
const PREFIX: &str = "launch2dashboard.";
const MAX_LOG_BYTES: u64 = 64 * 1024;

pub struct MacLaunchd {
    agents: PathBuf,
    logs_dir: PathBuf,
    domain: LaunchDomain,
    executor: Arc<dyn Executor>,
}
impl MacLaunchd {
    pub fn from_environment() -> Result<Self, AppError> {
        if !cfg!(target_os = "macos") {
            return Err(AppError::new(
                ErrorKind::Command,
                "launch2dashboard requires macOS launchd",
            ));
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| AppError::new(ErrorKind::Io, "HOME must be an absolute directory"))?;
        let executor = Arc::new(ProcessExecutor);
        let result = executor.run("/usr/bin/id", &["-u".into()])?;
        let uid = result
            .stdout
            .trim()
            .parse()
            .map_err(|_| AppError::new(ErrorKind::Command, "Could not determine user ID"))?;
        Self::new(
            home.join("Library/LaunchAgents"),
            home.join("Library/Logs/launch2dashboard"),
            uid,
            executor,
        )
    }
    fn new(
        agents: PathBuf,
        logs_dir: PathBuf,
        uid: u32,
        executor: Arc<dyn Executor>,
    ) -> Result<Self, AppError> {
        let domain = LaunchDomain::detect(uid, executor.as_ref())?;
        safe_directory(&agents)?;
        safe_directory(&logs_dir)?;
        Ok(Self {
            agents,
            logs_dir,
            domain,
            executor,
        })
    }
    fn path(&self, id: &str) -> Result<PathBuf, AppError> {
        validate_id(id)?;
        Ok(self.agents.join(format!("{}.plist", definition::label(id))))
    }
    fn target(&self, id: &str) -> String {
        format!("{}/{}", self.domain.target(), definition::label(id))
    }
    pub fn launch_domain(&self) -> String {
        self.domain.target()
    }
    fn call(&self, args: Vec<String>) -> Result<String, AppError> {
        let r = self.executor.run("/bin/launchctl", &args)?;
        if !r.success {
            return Err(AppError::new(
                ErrorKind::Command,
                format!("launchctl {} failed: {}", args[0], r.stderr.trim()),
            ));
        }
        Ok(r.stdout)
    }
    fn read(&self, id: &str) -> Result<Dictionary, AppError> {
        let path = self.path(id)?;
        safe_regular(&path, false)?;
        let value = Value::from_reader(open_read(&path)?)
            .map_err(|e| AppError::new(ErrorKind::Io, format!("{}: {e}", path.display())))?;
        let dict = value.into_dictionary().ok_or_else(|| {
            AppError::new(ErrorKind::Validation, "Plist root must be a dictionary")
        })?;
        if dict.get("Label").and_then(Value::as_string) != Some(definition::label(id).as_str()) {
            return Err(AppError::new(
                ErrorKind::Validation,
                format!("Plist label does not match service {id}"),
            ));
        }
        Ok(dict)
    }
    fn document(&self, c: &ServiceConfig, base: Dictionary) -> Dictionary {
        definition::to_plist(c, self.domain.session(), &self.logs_dir, base)
    }
    fn write(&self, id: &str, d: &Dictionary) -> Result<(), AppError> {
        let temp = self.agents.join(format!(
            ".{}.{}.tmp",
            definition::label(id),
            std::process::id()
        ));
        secure_fs::replace_private(&self.path(id)?, &temp, |file| {
            Value::Dictionary(d.clone())
                .to_writer_xml(file)
                .map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))
        })
    }
    fn job(&self, id: &str) -> Result<Option<JobStatus>, AppError> {
        let target = self.target(id);
        let result = self
            .executor
            .run("/bin/launchctl", &["print".into(), target.clone()])?;
        if result.success {
            return JobStatus::parse(&result.stdout).map(Some);
        }
        // ESRCH from launchctl service-target lookup; domain and permission failures are errors.
        if result.exit_code == Some(113) {
            return Ok(None);
        }
        Err(AppError::new(
            ErrorKind::Command,
            format!(
                "launchctl print {target} failed (exit {:?}): {}",
                result.exit_code,
                result.stderr.trim()
            ),
        ))
    }
    fn loaded(&self, id: &str) -> Result<bool, AppError> {
        Ok(self.job(id)?.is_some())
    }
    fn bootstrap(&self, id: &str) -> Result<(), AppError> {
        // Older L2D plists defaulted to Aqua, which cannot load in a headless user domain.
        let mut document = self.read(id)?;
        if document
            .get("LimitLoadToSessionType")
            .and_then(Value::as_string)
            != Some(self.domain.session())
        {
            document.insert(
                "LimitLoadToSessionType".into(),
                Value::String(self.domain.session().into()),
            );
            self.write(id, &document)?;
        }
        self.call(vec![
            "bootstrap".into(),
            self.domain.target(),
            self.path(id)?.to_string_lossy().into_owned(),
        ])
        .map(|_| ())
    }
    fn unload(&self, id: &str) -> Result<(), AppError> {
        if self.loaded(id)? {
            self.call(vec!["bootout".into(), self.target(id)])?;
        }
        Ok(())
    }
    fn uptime(&self, pid: u32) -> Option<u64> {
        self.executor
            .run(
                "/bin/ps",
                &["-o".into(), "etime=".into(), "-p".into(), pid.to_string()],
            )
            .ok()
            .filter(|r| r.success)
            .and_then(|r| status::parse_elapsed(r.stdout.trim()))
    }
    fn status(&self, id: &str) -> Result<ServiceStatus, AppError> {
        let job = self.job(id)?;
        let uptime = job
            .as_ref()
            .and_then(|job| job.pid)
            .and_then(|pid| self.uptime(pid));
        Ok(status::derive(job.as_ref(), uptime))
    }
    fn log_path(&self, id: &str, suffix: &str) -> PathBuf {
        definition::log_path(&self.logs_dir, id, suffix)
    }
    fn ensure_logs(&self, id: &str) -> Result<(), AppError> {
        for (_, suffix) in definition::LOG_FILES {
            secure_fs::touch_private(&self.log_path(id, suffix))?;
        }
        Ok(())
    }
}
impl ServiceManager for MacLaunchd {
    fn list(&self) -> Result<Vec<Service>, AppError> {
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.agents)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(id) = name
                .strip_prefix(PREFIX)
                .and_then(|n| n.strip_suffix(".plist"))
            {
                result.push(
                    self.get(id)
                        .unwrap_or_else(|error| Service::unreadable(id, error.message)),
                );
            }
        }
        result.sort_by(|a, b| a.config.id.cmp(&b.config.id));
        Ok(result)
    }
    fn get(&self, id: &str) -> Result<Service, AppError> {
        let d = self.read(id)?;
        Ok(Service {
            config: definition::to_config(id, &d)?,
            status: self.status(id)?,
        })
    }
    fn create(&self, config: ServiceConfig) -> Result<Service, AppError> {
        config.validate()?;
        let path = self.path(&config.id)?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(AppError::new(ErrorKind::Conflict, "Service already exists"));
        }
        self.ensure_logs(&config.id)?;
        self.write(&config.id, &self.document(&config, Dictionary::new()))?;
        if let Err(e) = self.bootstrap(&config.id) {
            let _ = fs::remove_file(path);
            return Err(e);
        }
        self.get(&config.id)
    }
    fn update(&self, id: &str, config: ServiceConfig) -> Result<Service, AppError> {
        config.validate()?;
        validate_id(id)?;
        if config.id != id {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Renaming a service is not supported",
            ));
        }
        let previous = self.read(id)?;
        definition::check_editable(&previous)?;
        let was_running = self.status(id)?.pid.is_some();
        self.unload(id)?;
        let change: Result<(), AppError> = (|| {
            self.ensure_logs(id)?;
            self.write(id, &self.document(&config, previous.clone()))?;
            if was_running {
                self.bootstrap(id)?;
                self.call(vec!["kickstart".into(), self.target(id)])?;
            }
            Ok(())
        })();
        if let Err(error) = change {
            let restore = self
                .unload(id)
                .and_then(|_| self.write(id, &previous))
                .and_then(|_| {
                    if was_running {
                        self.bootstrap(id)?;
                        self.call(vec!["kickstart".into(), self.target(id)])
                            .map(|_| ())
                    } else {
                        Ok(())
                    }
                });
            return Err(AppError::new(
                error.kind,
                format!(
                    "{}; rollback {}",
                    error.message,
                    restore
                        .map(|_| "completed".to_owned())
                        .unwrap_or_else(|e| format!("failed: {e}"))
                ),
            ));
        }
        self.get(id)
    }
    fn delete(&self, id: &str) -> Result<(), AppError> {
        self.read(id)?;
        self.unload(id)?;
        fs::remove_file(self.path(id)?)?;
        Ok(())
    }
    fn action(&self, id: &str, action: ServiceAction) -> Result<Service, AppError> {
        self.read(id)?;
        match action {
            ServiceAction::Stop => self.unload(id)?,
            ServiceAction::Start | ServiceAction::Restart => {
                if !self.loaded(id)? {
                    self.bootstrap(id)?;
                }
                let mut args = vec!["kickstart".into()];
                if action == ServiceAction::Restart {
                    args.push("-k".into());
                }
                args.push(self.target(id));
                self.call(args)?;
            }
        }
        self.get(id)
    }
    fn logs(&self, id: &str) -> Result<LogSnapshot, AppError> {
        let doc = self.read(id)?;
        let read = |key: &str, suffix: &str| -> Result<String, AppError> {
            let path = self.log_path(id, suffix);
            if doc.get(key).and_then(Value::as_string) != path.to_str() {
                return Err(AppError::new(
                    ErrorKind::Validation,
                    "Logs outside the launch2dashboard log directory cannot be read",
                ));
            }
            secure_fs::tail(&path, MAX_LOG_BYTES)
        };
        let [(out_key, out_suffix), (err_key, err_suffix)] = definition::LOG_FILES;
        Ok(LogSnapshot {
            stdout: read(out_key, out_suffix)?,
            stderr: read(err_key, err_suffix)?,
        })
    }
}
