use super::executor::{CommandResult, Executor};
use crate::domain::{AppError, ErrorKind};
use std::{collections::VecDeque, sync::Arc, sync::Mutex};
/// Replays an exact sequence of launchctl invocations and fails on any deviation.
pub(super) struct Scripted {
    pub(super) steps: Mutex<VecDeque<(Vec<String>, CommandResult)>>,
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
/// The kind of an expected failure; works for success types without `Debug`.
pub(super) fn error_kind<T>(result: Result<T, AppError>) -> ErrorKind {
    result.err().expect("expected an error").kind
}
pub(super) fn response(exit: i32, stdout: &str) -> CommandResult {
    CommandResult {
        success: exit == 0,
        exit_code: Some(exit),
        stdout: stdout.into(),
        stderr: format!("error {exit}"),
    }
}
pub(super) fn scripted(steps: Vec<(&[&str], CommandResult)>) -> Arc<Scripted> {
    Arc::new(Scripted {
        steps: Mutex::new(
            steps
                .into_iter()
                .map(|(args, result)| (args.iter().map(|s| s.to_string()).collect(), result))
                .collect(),
        ),
    })
}
