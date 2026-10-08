use crate::domain::{AppError, ErrorKind, ServiceState, ServiceStatus};
#[derive(Default)]
pub(super) struct JobStatus {
    pub(super) pid: Option<u32>,
    pub(super) exit_code: Option<i32>,
    pub(super) terminating_signal: Option<String>,
}
impl JobStatus {
    pub(super) fn parse(output: &str) -> Result<Self, AppError> {
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
/// Derives the dashboard status from a launchd job (`None` when not loaded).
pub(super) fn derive(job: Option<&JobStatus>, uptime_seconds: Option<u64>) -> ServiceStatus {
    let pid = job.and_then(|job| job.pid);
    let signal = job.and_then(|job| job.terminating_signal.as_deref());
    let exit = job.and_then(|job| job.exit_code).or_else(|| {
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
    ServiceStatus {
        state: if pid.is_some() {
            ServiceState::Running
        } else if signal.is_some() || exit.is_some_and(|e| e != 0) {
            ServiceState::Error
        } else {
            ServiceState::Stopped
        },
        pid,
        uptime_seconds: pid.and(uptime_seconds),
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
    }
}
/// Parses `ps -o etime=` output: `[[dd-]hh:]mm:ss`.
pub(super) fn parse_elapsed(value: &str) -> Option<u64> {
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

#[cfg(test)]
mod tests {
    use super::super::testing::error_kind;
    use super::*;
    #[test]
    fn print_parser_does_not_treat_literal_braces_as_structure() {
        let output = "user/501/demo = {\n\targuments = {\n\t\t/bin/echo\n\t\t{\n\t}\n\tenvironment = {\n\t\tLITERAL => {\n\t}\n\tstate = running\n\tpid = 42\n\tlast exit code = 0\n}";
        let parsed = JobStatus::parse(output).unwrap();
        assert_eq!(parsed.pid, Some(42));
        assert_eq!(parsed.exit_code, Some(0));
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
        assert_eq!(
            error_kind(JobStatus::parse("unexpected output")),
            ErrorKind::Command
        );
        assert_eq!(
            error_kind(JobStatus::parse(
                "demo = {\n\tenvironment = {\n\t\tstate = running\n\t}\n}"
            )),
            ErrorKind::Command
        );
    }
    #[test]
    fn signal_termination_is_error_not_stopped() {
        let job = JobStatus::parse(
            "gui/501/demo = {\n\tstate = not running\n\tlast terminating signal = Terminated: 15\n}",
        )
        .unwrap();
        let status = derive(Some(&job), None);
        assert_eq!(status.state, ServiceState::Error);
        assert_eq!(status.last_exit_code, Some(-15));
        assert!(status.error.unwrap().contains("Terminated: 15"));
    }
    #[test]
    fn running_unloaded_and_failed_jobs_map_to_distinct_states() {
        let running = JobStatus {
            pid: Some(42),
            exit_code: Some(1),
            terminating_signal: None,
        };
        let status = derive(Some(&running), Some(7));
        assert_eq!(status.state, ServiceState::Running);
        assert_eq!(status.uptime_seconds, Some(7));
        assert_eq!(status.error, None);
        let unloaded = derive(None, Some(7));
        assert_eq!(unloaded.state, ServiceState::Stopped);
        assert_eq!(unloaded.uptime_seconds, None);
        let failed = JobStatus {
            exit_code: Some(3),
            ..JobStatus::default()
        };
        let status = derive(Some(&failed), None);
        assert_eq!(status.state, ServiceState::Error);
        assert_eq!(status.error.as_deref(), Some("Last process exit: 3"));
    }
    #[test]
    fn elapsed_parser() {
        assert_eq!(parse_elapsed("2-03:04:05"), Some(183845));
        assert_eq!(parse_elapsed("04:05"), Some(245));
        assert_eq!(parse_elapsed("oops"), None);
    }
}
