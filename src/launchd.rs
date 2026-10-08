use crate::domain::*;
use plist::{Dictionary, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
const PREFIX: &str = "launch2dashboard.";
const MAX_LOG_BYTES: u64 = 64 * 1024;
struct CommandResult {
    success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}
trait Executor: Send + Sync {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandResult, AppError>;
}
struct ProcessExecutor;
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
#[derive(Clone, Copy)]
enum LaunchDomain {
    Gui(u32),
    User(u32),
}
impl LaunchDomain {
    fn target(self) -> String {
        match self {
            Self::Gui(uid) => format!("gui/{uid}"),
            Self::User(uid) => format!("user/{uid}"),
        }
    }
    fn session(self) -> &'static str {
        match self {
            Self::Gui(_) => "Aqua",
            Self::User(_) => "Background",
        }
    }
    fn detect(uid: u32, executor: &dyn Executor) -> Result<Self, AppError> {
        let gui = Self::Gui(uid);
        let result = executor.run("/bin/launchctl", &["print".into(), gui.target()])?;
        if result.success {
            return Ok(gui);
        }
        let user = Self::User(uid);
        let background = executor.run("/bin/launchctl", &["print".into(), user.target()])?;
        if background.success {
            return Ok(user);
        }
        Err(AppError::new(
            ErrorKind::Command,
            format!(
                "No accessible launchd domain: {}: {}; {}: {}",
                gui.target(),
                result.stderr.trim(),
                user.target(),
                background.stderr.trim()
            ),
        ))
    }
}

#[derive(Default)]
struct JobStatus {
    pid: Option<u32>,
    exit_code: Option<i32>,
    terminating_signal: Option<String>,
}
impl JobStatus {
    fn parse(output: &str) -> Result<Self, AppError> {
        let mut result = Self::default();
        let mut state = None;
        // launchctl indents service fields with one tab and nested dictionaries / arguments
        // with additional tabs. Values may contain literal braces; never count their braces.
        for line in output.lines() {
            let Some(line) = line
                .strip_prefix('\t')
                .filter(|line| !line.starts_with('\t'))
            else {
                continue;
            };
            if let Some((key, value)) = line.split_once(" = ") {
                match key {
                    "state" => state = Some(value),
                    "pid" => result.pid = value.parse().ok().filter(|pid| *pid > 0),
                    "last exit code" => result.exit_code = value.parse().ok(),
                    "last terminating signal" => result.terminating_signal = Some(value.to_owned()),
                    _ => {}
                }
            }
        }
        if state.is_none() {
            return Err(AppError::new(
                ErrorKind::Command,
                "Unrecognized launchctl print service output",
            ));
        }
        Ok(result)
    }
}

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
        Ok(self.agents.join(format!("{PREFIX}{id}.plist")))
    }
    fn target(&self, id: &str) -> String {
        format!("{}/{PREFIX}{id}", self.domain.target())
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
        if dict.get("Label").and_then(Value::as_string) != Some(format!("{PREFIX}{id}").as_str()) {
            return Err(AppError::new(
                ErrorKind::Validation,
                format!("Plist label does not match service {id}"),
            ));
        }
        Ok(dict)
    }
    fn config(&self, id: &str, d: &Dictionary) -> Result<ServiceConfig, AppError> {
        let invalid = || {
            AppError::new(
                ErrorKind::Validation,
                format!(
                    "Unsupported configuration for {id}: expected ProgramArguments and boolean RunAtLoad / KeepAlive.SuccessfulExit"
                ),
            )
        };
        let args = d
            .get("ProgramArguments")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?
            .iter()
            .map(|v| v.as_string().map(str::to_owned).ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        let (first, rest) = args.split_first().ok_or_else(invalid)?;
        // Program overrides argv[0] in launchd, preserve its actual executable for display.
        let executable = match d.get("Program") {
            Some(v) => v.as_string().ok_or_else(invalid)?.to_owned(),
            None => first.clone(),
        };
        let environment = match d.get("EnvironmentVariables") {
            None => BTreeMap::new(),
            Some(v) => v
                .as_dictionary()
                .ok_or_else(invalid)?
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.as_string().ok_or_else(invalid)?.to_owned())))
                .collect::<Result<_, AppError>>()?,
        };
        let autostart = match d.get("RunAtLoad") {
            None => false,
            Some(v) => v.as_boolean().ok_or_else(invalid)?,
        };
        let restart_on_failure = match d.get("KeepAlive") {
            None => false,
            Some(Value::Boolean(v)) => *v,
            Some(v) => {
                v.as_dictionary()
                    .and_then(|d| d.get("SuccessfulExit"))
                    .and_then(Value::as_boolean)
                    == Some(false)
            }
        };
        let working_directory = match d.get("WorkingDirectory") {
            None => None,
            Some(v) => Some(v.as_string().ok_or_else(invalid)?.to_owned()),
        };
        Ok(ServiceConfig {
            id: id.into(),
            executable,
            arguments: rest.to_vec(),
            working_directory,
            environment,
            autostart,
            restart_on_failure,
        })
    }
    fn document(&self, c: &ServiceConfig, mut d: Dictionary) -> Dictionary {
        d.insert("Label".into(), Value::String(format!("{PREFIX}{}", c.id)));
        d.remove("Program");
        d.insert(
            "LimitLoadToSessionType".into(),
            Value::String(self.domain.session().into()),
        );
        d.insert(
            "ProgramArguments".into(),
            Value::Array(
                std::iter::once(&c.executable)
                    .chain(c.arguments.iter())
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        if let Some(w) = &c.working_directory {
            d.insert("WorkingDirectory".into(), Value::String(w.clone()));
        } else {
            d.remove("WorkingDirectory");
        }
        d.insert(
            "EnvironmentVariables".into(),
            Value::Dictionary(
                c.environment
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ),
        );
        d.insert("RunAtLoad".into(), Value::Boolean(c.autostart));
        if c.restart_on_failure {
            let mut k = Dictionary::new();
            k.insert("SuccessfulExit".into(), Value::Boolean(false));
            d.insert("KeepAlive".into(), Value::Dictionary(k));
        } else {
            d.insert("KeepAlive".into(), Value::Boolean(false));
        }
        for (key, suffix) in [("StandardOutPath", "log"), ("StandardErrorPath", "err.log")] {
            d.insert(
                key.into(),
                Value::String(
                    self.logs_dir
                        .join(format!("{}.{}", c.id, suffix))
                        .to_string_lossy()
                        .into_owned(),
                ),
            );
        }
        d
    }
    fn write(&self, id: &str, d: &Dictionary) -> Result<(), AppError> {
        let path = self.path(id)?;
        safe_regular(&path, true)?;
        let temp = self
            .agents
            .join(format!(".{PREFIX}{id}.{}.tmp", std::process::id()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            Value::Dictionary(d.clone())
                .to_writer_xml(&mut file)
                .map_err(|e| AppError::new(ErrorKind::Io, e.to_string()))?;
            file.sync_all()?;
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
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
    fn status(&self, id: &str) -> Result<ServiceStatus, AppError> {
        let job = self.job(id)?;
        let pid = job.as_ref().and_then(|job| job.pid);
        let signal = job
            .as_ref()
            .and_then(|job| job.terminating_signal.as_deref());
        let exit = job.as_ref().and_then(|job| job.exit_code).or_else(|| {
            signal
                .and_then(|value| {
                    value
                        .rsplit_once(':')
                        .map_or(value, |(_, number)| number)
                        .trim()
                        .parse::<i32>()
                        .ok()
                })
                .filter(|value| *value > 0)
                .map(|value| -value)
        });
        let uptime_seconds = pid.and_then(|pid| {
            self.executor
                .run(
                    "/bin/ps",
                    &["-o".into(), "etime=".into(), "-p".into(), pid.to_string()],
                )
                .ok()
                .filter(|r| r.success)
                .and_then(|r| parse_elapsed(r.stdout.trim()))
        });
        Ok(ServiceStatus {
            state: if pid.is_some() {
                ServiceState::Running
            } else if signal.is_some() || exit.is_some_and(|e| e != 0) {
                ServiceState::Error
            } else {
                ServiceState::Stopped
            },
            pid,
            uptime_seconds,
            restart_count: None,
            last_exit_code: exit,
            error: if pid.is_some() {
                None
            } else if let Some(signal) = signal {
                Some(format!("Last process terminated by signal: {signal}"))
            } else {
                exit.filter(|e| *e != 0)
                    .map(|e| format!("Last process exit: {e}"))
            },
        })
    }
    fn ensure_logs(&self, id: &str) -> Result<(), AppError> {
        for suffix in ["log", "err.log"] {
            let p = self.logs_dir.join(format!("{id}.{suffix}"));
            safe_regular(&p, true)?;
            OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .custom_flags(no_follow())
                .open(p)?;
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
                result.push(self.get(id).unwrap_or_else(|error| Service {
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
                        error: Some(error.message),
                    },
                }));
            }
        }
        result.sort_by(|a, b| a.config.id.cmp(&b.config.id));
        Ok(result)
    }
    fn get(&self, id: &str) -> Result<Service, AppError> {
        let d = self.read(id)?;
        Ok(Service {
            config: self.config(id, &d)?,
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
        let supported_keepalive = match previous.get("KeepAlive") {
            None | Some(Value::Boolean(false)) => true,
            Some(Value::Dictionary(k)) => {
                k.len() == 1 && k.get("SuccessfulExit").and_then(Value::as_boolean) == Some(false)
            }
            _ => false,
        };
        if !supported_keepalive {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Cannot edit this service: unsupported KeepAlive policy",
            ));
        }
        if previous.contains_key("Program") {
            return Err(AppError::new(
                ErrorKind::Validation,
                "Cannot edit a service with a separate Program key",
            ));
        }
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
    fn action(&self, id: &str, action: &str) -> Result<Service, AppError> {
        self.read(id)?;
        match action {
            "stop" => self.unload(id)?,
            "start" | "restart" => {
                if !self.loaded(id)? {
                    self.bootstrap(id)?;
                }
                let mut args = vec!["kickstart".into()];
                if action == "restart" {
                    args.push("-k".into());
                }
                args.push(self.target(id));
                self.call(args)?;
            }
            _ => {
                return Err(AppError::new(
                    ErrorKind::Validation,
                    "Unknown service action",
                ));
            }
        }
        self.get(id)
    }
    fn logs(&self, id: &str) -> Result<LogSnapshot, AppError> {
        let doc = self.read(id)?;
        let read = |key: &str, suffix: &str| -> Result<String, AppError> {
            let path = self.logs_dir.join(format!("{id}.{suffix}"));
            if doc.get(key).and_then(Value::as_string) != path.to_str() {
                return Err(AppError::new(
                    ErrorKind::Validation,
                    "Logs outside the launch2dashboard log directory cannot be read",
                ));
            }
            safe_regular(&path, true)?;
            let mut f = match open_read(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
                Err(e) => return Err(e.into()),
            };
            let size = f.metadata()?.len();
            f.seek(SeekFrom::Start(size.saturating_sub(MAX_LOG_BYTES)))?;
            let mut bytes = Vec::new();
            f.take(MAX_LOG_BYTES).read_to_end(&mut bytes)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        };
        Ok(LogSnapshot {
            stdout: read("StandardOutPath", "log")?,
            stderr: read("StandardErrorPath", "err.log")?,
        })
    }
}
fn safe_directory(path: &Path) -> Result<(), AppError> {
    for ancestor in path.ancestors() {
        if let Ok(m) = fs::symlink_metadata(ancestor)
            && m.file_type().is_symlink()
        {
            return Err(AppError::new(
                ErrorKind::Validation,
                format!(
                    "Symlink directories are not supported: {}",
                    ancestor.display()
                ),
            ));
        }
    }
    fs::create_dir_all(path)?;
    if !path.is_dir() {
        return Err(AppError::new(ErrorKind::Io, "Expected a directory"));
    }
    Ok(())
}
fn safe_regular(path: &Path, missing_ok: bool) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(AppError::new(
            ErrorKind::Validation,
            format!("Refusing non-regular file: {}", path.display()),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing_ok => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(AppError::new(ErrorKind::NotFound, "Service not found"))
        }
        Err(e) => Err(e.into()),
    }
}
fn parse_elapsed(value: &str) -> Option<u64> {
    let (days, time) = if let Some((d, t)) = value.split_once('-') {
        (d.parse::<u64>().ok()?, t)
    } else {
        (0, value)
    };
    let fields = time
        .split(':')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let seconds = match fields.as_slice() {
        [m, s] => m.checked_mul(60)?.checked_add(*s)?,
        [h, m, s] => h
            .checked_mul(3600)?
            .checked_add(m.checked_mul(60)?)?
            .checked_add(*s)?,
        _ => return None,
    };
    days.checked_mul(86400)?.checked_add(seconds)
}

fn no_follow() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x20000
    }
}
fn open_read(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(no_follow())
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        fail_bootstrap: Mutex<usize>,
        running: Mutex<bool>,
    }
    impl Executor for Fake {
        fn run(&self, _program: &str, args: &[String]) -> Result<CommandResult, AppError> {
            self.calls.lock().unwrap().push(args.to_vec());
            let mut fail = self.fail_bootstrap.lock().unwrap();
            if args[0] == "bootstrap" && *fail > 0 {
                *fail -= 1;
                return Err(AppError::new(
                    ErrorKind::Command,
                    "simulated bootstrap failure",
                ));
            }
            Ok(CommandResult {
                success: true,
                exit_code: Some(0),
                stdout: if args[0] == "print" {
                    if *self.running.lock().unwrap() {
                        "service = {\n\tstate = running\n\tpid = 42\n\tlast exit code = 0\n}".into()
                    } else {
                        "service = {\n\tstate = not running\n\tlast exit code = 0\n}".into()
                    }
                } else {
                    String::new()
                },
                stderr: String::new(),
            })
        }
    }
    fn fixture() -> (tempfile::TempDir, MacLaunchd, Arc<Fake>) {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(Fake::default());
        let m = MacLaunchd::new(
            dir.path().canonicalize().unwrap().join("agents"),
            dir.path().canonicalize().unwrap().join("logs"),
            501,
            fake.clone(),
        )
        .unwrap();
        fake.calls.lock().unwrap().clear();
        (dir, m, fake)
    }
    fn config() -> ServiceConfig {
        ServiceConfig {
            id: "demo".into(),
            executable: "/bin/echo".into(),
            arguments: vec!["hello <world>".into()],
            working_directory: None,
            environment: BTreeMap::new(),
            autostart: true,
            restart_on_failure: true,
        }
    }
    struct Headless;
    impl Executor for Headless {
        fn run(&self, _program: &str, args: &[String]) -> Result<CommandResult, AppError> {
            Ok(CommandResult {
                success: args != ["print", "gui/501"],
                exit_code: Some(if args == ["print", "gui/501"] { 125 } else { 0 }),
                stdout: String::new(),
                stderr: "Domain does not support specified action".into(),
            })
        }
    }
    #[test]
    fn ssh_without_gui_uses_user_domain() {
        let dir = tempfile::tempdir().unwrap();
        let manager = MacLaunchd::new(
            dir.path().canonicalize().unwrap().join("agents"),
            dir.path().canonicalize().unwrap().join("logs"),
            501,
            Arc::new(Headless),
        )
        .unwrap();
        assert_eq!(manager.target("demo"), "user/501/launch2dashboard.demo");
        assert_eq!(
            manager
                .document(&config(), Dictionary::new())
                .get("LimitLoadToSessionType")
                .and_then(Value::as_string),
            Some("Background")
        );
    }
    struct Scripted {
        steps: Mutex<std::collections::VecDeque<(Vec<String>, CommandResult)>>,
    }
    impl Executor for Scripted {
        fn run(&self, program: &str, args: &[String]) -> Result<CommandResult, AppError> {
            assert_eq!(program, "/bin/launchctl");
            let (expected, result) = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected command");
            assert_eq!(args, expected);
            Ok(result)
        }
    }
    fn response(exit: i32, stdout: &str) -> CommandResult {
        CommandResult {
            success: exit == 0,
            exit_code: Some(exit),
            stdout: stdout.into(),
            stderr: format!("error {exit}"),
        }
    }
    fn scripted(steps: Vec<(&[&str], CommandResult)>) -> Arc<Scripted> {
        Arc::new(Scripted {
            steps: Mutex::new(
                steps
                    .into_iter()
                    .map(|(args, result)| (args.iter().map(|s| s.to_string()).collect(), result))
                    .collect(),
            ),
        })
    }
    #[test]
    fn domain_prefers_gui_and_reports_failure_of_both_domains() {
        let executor = scripted(vec![(&["print", "gui/501"], response(0, ""))]);
        assert_eq!(
            LaunchDomain::detect(501, executor.as_ref())
                .unwrap()
                .target(),
            "gui/501"
        );
        let executor = scripted(vec![
            (&["print", "gui/501"], response(125, "")),
            (&["print", "user/501"], response(1, "")),
        ]);
        let error = LaunchDomain::detect(501, executor.as_ref()).err().unwrap();
        assert!(error.message.contains("gui/501: error 125"));
        assert!(error.message.contains("user/501: error 1"));
    }
    #[test]
    fn print_parser_does_not_treat_literal_braces_as_structure() {
        let output = "user/501/demo = {\n\targuments = {\n\t\t/bin/echo\n\t\t{\n\t}\n\tenvironment = {\n\t\tLITERAL => {\n\t}\n\tstate = running\n\tpid = 42\n\tlast exit code = 0\n}";
        let parsed = JobStatus::parse(output).unwrap();
        assert_eq!(parsed.pid, Some(42));
        assert_eq!(parsed.exit_code, Some(0));
    }
    #[test]
    fn signal_termination_is_error_not_stopped() {
        let (_dir, mut manager, _) = fixture();
        manager.executor = scripted(vec![(
            &["print", "gui/501/launch2dashboard.demo"],
            response(
                0,
                "gui/501/demo = {\n\tstate = not running\n\tlast terminating signal = Terminated: 15\n}",
            ),
        )]);
        let status = manager.status("demo").unwrap();
        assert_eq!(status.state, ServiceState::Error);
        assert_eq!(status.last_exit_code, Some(-15));
        assert!(status.error.unwrap().contains("Terminated: 15"));
    }
    #[test]
    fn print_parser_uses_only_service_fields() {
        let result = JobStatus::parse("user/501/demo = {\n\tstate = xpcproxy\n\tpid = 57894\n\tlast exit code = (never exited)\n\tenvironment = {\n\t\tpid = 999\n\t\tlast exit code = 9\n\t}\n}").unwrap();
        assert_eq!(result.pid, Some(57894));
        assert_eq!(result.exit_code, None);
        let result =
            JobStatus::parse("user/501/demo = {\n\tstate = not running\n\tlast exit code = 7\n}")
                .unwrap();
        assert_eq!(result.pid, None);
        assert_eq!(result.exit_code, Some(7));
        assert!(JobStatus::parse("unexpected output").is_err());
        assert!(
            JobStatus::parse("demo = {\n\tenvironment = {\n\t\tstate = running\n\t}\n}").is_err()
        );
    }
    #[test]
    fn service_absence_is_distinct_from_domain_and_permission_errors() {
        let (_dir, mut manager, _) = fixture();
        for code in [113, 125, 1] {
            manager.executor = scripted(vec![(
                &["print", "gui/501/launch2dashboard.demo"],
                response(code, ""),
            )]);
            let result = manager.job("demo");
            if code == 113 {
                assert!(result.unwrap().is_none());
            } else {
                assert!(result.is_err());
            }
        }
    }
    #[test]
    fn ssh_mutations_and_status_keep_same_domain_and_upgrade_old_plist() {
        let (_dir, mut manager, _) = fixture();
        manager.domain = LaunchDomain::User(501);
        let mut doc = manager.document(&config(), Dictionary::new());
        doc.remove("LimitLoadToSessionType");
        doc.insert("ThrottleInterval".into(), Value::Integer(17.into()));
        manager.write("demo", &doc).unwrap();
        let path = manager.path("demo").unwrap().to_string_lossy().into_owned();
        let output =
            "user/501/launch2dashboard.demo = {\n\tstate = not running\n\tlast exit code = 0\n}";
        let executor = scripted(vec![
            (
                &["print", "user/501/launch2dashboard.demo"],
                response(113, ""),
            ),
            (&["bootstrap", "user/501", &path], response(0, "")),
            (
                &["kickstart", "user/501/launch2dashboard.demo"],
                response(0, ""),
            ),
            (
                &["print", "user/501/launch2dashboard.demo"],
                response(0, output),
            ),
            (
                &["print", "user/501/launch2dashboard.demo"],
                response(0, output),
            ),
            (
                &["bootout", "user/501/launch2dashboard.demo"],
                response(0, ""),
            ),
            (
                &["print", "user/501/launch2dashboard.demo"],
                response(113, ""),
            ),
        ]);
        manager.executor = executor.clone();
        manager.action("demo", "start").unwrap();
        manager.action("demo", "stop").unwrap();
        assert!(executor.steps.lock().unwrap().is_empty());
        let written = manager.read("demo").unwrap();
        assert_eq!(
            written
                .get("LimitLoadToSessionType")
                .and_then(Value::as_string),
            Some("Background")
        );
        assert_eq!(written.get("ThrottleInterval"), doc.get("ThrottleInterval"));
    }
    #[test]
    fn failed_background_bootstrap_does_not_retry_another_domain() {
        let (_dir, mut manager, _) = fixture();
        manager.domain = LaunchDomain::User(501);
        let path = manager.path("demo").unwrap().to_string_lossy().into_owned();
        let executor = scripted(vec![(&["bootstrap", "user/501", &path], response(5, ""))]);
        manager.executor = executor.clone();
        assert!(manager.create(config()).is_err());
        assert!(!manager.path("demo").unwrap().exists());
        assert!(executor.steps.lock().unwrap().is_empty());
    }
    #[test]
    fn create_roundtrip_and_stop_boots_out() {
        let (_d, m, f) = fixture();
        let c = config();
        assert_eq!(m.create(c.clone()).unwrap().config, c);
        m.action("demo", "stop").unwrap();
        assert!(
            f.calls
                .lock()
                .unwrap()
                .iter()
                .any(|a| a == &["bootout", "gui/501/launch2dashboard.demo"])
        );
    }
    #[test]
    fn invalid_input_has_no_side_effect() {
        let (_d, m, f) = fixture();
        let mut c = config();
        c.id = "../x".into();
        assert!(m.create(c).is_err());
        assert!(f.calls.lock().unwrap().is_empty());
        assert_eq!(fs::read_dir(&m.agents).unwrap().count(), 0);
    }
    #[test]
    fn failed_create_removes_plist() {
        let (_d, m, f) = fixture();
        *f.fail_bootstrap.lock().unwrap() = 1;
        assert!(m.create(config()).is_err());
        assert!(!m.path("demo").unwrap().exists());
    }
    #[test]
    fn new_plists_and_logs_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, manager, _) = fixture();
        manager.create(config()).unwrap();
        for path in [
            manager.path("demo").unwrap(),
            manager.logs_dir.join("demo.log"),
            manager.logs_dir.join("demo.err.log"),
        ] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn update_preserves_unknown_keys_and_rolls_back() {
        let (_d, m, f) = fixture();
        *f.running.lock().unwrap() = true;
        m.create(config()).unwrap();
        let mut doc = m.read("demo").unwrap();
        doc.insert("ThrottleInterval".into(), Value::Integer(17.into()));
        m.write("demo", &doc).unwrap();
        let mut c = config();
        c.arguments = vec!["changed".into()];
        m.update("demo", c.clone()).unwrap();
        assert_eq!(
            m.read("demo")
                .unwrap()
                .get("ThrottleInterval")
                .unwrap()
                .as_signed_integer(),
            Some(17)
        );
        *f.fail_bootstrap.lock().unwrap() = 1;
        c.arguments = vec!["failed".into()];
        assert!(m.update("demo", c).is_err());
        assert_eq!(m.get("demo").unwrap().config.arguments, vec!["changed"]);
    }
    #[test]
    fn log_tail_bounded_and_external_paths_rejected() {
        let (_d, m, _) = fixture();
        m.create(config()).unwrap();
        fs::write(m.logs_dir.join("demo.log"), vec![b'x'; 100_000]).unwrap();
        assert_eq!(m.logs("demo").unwrap().stdout.len(), MAX_LOG_BYTES as usize);
        let mut doc = m.read("demo").unwrap();
        doc.insert(
            "StandardOutPath".into(),
            Value::String("/etc/passwd".into()),
        );
        m.write("demo", &doc).unwrap();
        assert!(m.logs("demo").is_err());
    }
    #[test]
    fn rejects_symlink_plist() {
        use std::os::unix::fs::symlink;
        let (d, m, _) = fixture();
        let outside = d.path().join("outside");
        fs::write(&outside, "secret").unwrap();
        symlink(outside, m.path("demo").unwrap()).unwrap();
        assert!(m.get("demo").is_err());
    }
    #[test]
    fn elapsed_parser() {
        assert_eq!(parse_elapsed("2-03:04:05"), Some(183845));
        assert_eq!(parse_elapsed("04:05"), Some(245));
        assert_eq!(parse_elapsed("oops"), None);
    }
    #[test]
    fn list_ignores_other_namespaces() {
        let (_d, manager, _) = fixture();
        fs::write(
            manager.agents.join("com.example.other.plist"),
            "invalid xml",
        )
        .unwrap();
        manager.create(config()).unwrap();
        assert_eq!(manager.list().unwrap().len(), 1);
    }
    #[test]
    fn logs_reject_symlink_without_reading_target() {
        use std::os::unix::fs::symlink;
        let (dir, manager, _) = fixture();
        manager.create(config()).unwrap();
        let path = manager.logs_dir.join("demo.log");
        fs::remove_file(&path).unwrap();
        let secret = dir.path().join("secret");
        fs::write(&secret, "private").unwrap();
        symlink(secret, path).unwrap();
        assert!(manager.logs("demo").is_err());
    }
    #[test]
    fn unsupported_keepalive_is_not_silently_lost() {
        let (_d, manager, fake) = fixture();
        manager.create(config()).unwrap();
        let mut doc = manager.read("demo").unwrap();
        let mut keepalive = Dictionary::new();
        keepalive.insert("NetworkState".into(), Value::Boolean(true));
        doc.insert("KeepAlive".into(), Value::Dictionary(keepalive));
        manager.write("demo", &doc).unwrap();
        fake.calls.lock().unwrap().clear();
        assert!(manager.update("demo", config()).is_err());
        assert!(fake.calls.lock().unwrap().is_empty());
        assert_eq!(manager.read("demo").unwrap(), doc);
    }
    #[test]
    fn update_stopped_does_not_restart_and_manual_running_is_restarted() {
        let (_d, m, f) = fixture();
        m.create(config()).unwrap();
        f.calls.lock().unwrap().clear();
        m.update("demo", config()).unwrap();
        assert!(
            !f.calls
                .lock()
                .unwrap()
                .iter()
                .any(|a| a[0] == "bootstrap" || a[0] == "kickstart")
        );
        *f.running.lock().unwrap() = true;
        let mut c = config();
        c.autostart = false;
        c.restart_on_failure = false;
        m.update("demo", c).unwrap();
        assert!(f.calls.lock().unwrap().iter().any(|a| a[0] == "kickstart"));
    }
    #[test]
    fn invalid_plist_does_not_hide_healthy_services() {
        let (_d, m, _) = fixture();
        m.create(config()).unwrap();
        fs::write(m.path("broken").unwrap(), "invalid").unwrap();
        let services = m.list().unwrap();
        assert_eq!(services.len(), 2);
        assert!(
            services
                .iter()
                .any(|s| s.config.id == "broken" && s.status.state == ServiceState::Error)
        );
        assert!(
            services
                .iter()
                .any(|s| s.config.id == "demo" && s.status.state != ServiceState::Error)
        );
    }
    #[test]
    fn keepalive_true_cannot_be_silently_changed() {
        let (_d, m, _) = fixture();
        m.create(config()).unwrap();
        for value in [
            Value::Boolean(true),
            Value::Dictionary(
                [(String::from("SuccessfulExit"), Value::Boolean(true))]
                    .into_iter()
                    .collect(),
            ),
        ] {
            let mut d = m.read("demo").unwrap();
            d.insert("KeepAlive".into(), value);
            m.write("demo", &d).unwrap();
            assert!(m.update("demo", config()).is_err());
        }
    }
}
