use super::executor::{CommandResult, Executor};
use super::testing::{error_kind, response, scripted};
use super::*;
use std::{collections::BTreeMap, sync::Mutex};
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
#[test]
fn ssh_without_gui_manager_targets_user_domain_and_background_session() {
    let dir = tempfile::tempdir().unwrap();
    let manager = MacLaunchd::new(
        dir.path().canonicalize().unwrap().join("agents"),
        dir.path().canonicalize().unwrap().join("logs"),
        501,
        scripted(vec![
            (&["print", "gui/501"], response(125, "")),
            (&["print", "user/501"], response(0, "")),
        ]),
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
#[test]
fn signal_terminated_job_reports_error_without_querying_uptime() {
    let (_dir, mut manager, _) = fixture();
    // Scripted rejects any non-launchctl program, so a `ps` call would fail the test.
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
            assert_eq!(error_kind(result), ErrorKind::Command);
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
    manager.action("demo", ServiceAction::Start).unwrap();
    manager.action("demo", ServiceAction::Stop).unwrap();
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
    assert_eq!(error_kind(manager.create(config())), ErrorKind::Command);
    assert!(!manager.path("demo").unwrap().exists());
    assert!(executor.steps.lock().unwrap().is_empty());
}
#[test]
fn create_roundtrip_and_stop_boots_out() {
    let (_d, m, f) = fixture();
    let c = config();
    assert_eq!(m.create(c.clone()).unwrap().config, c);
    m.action("demo", ServiceAction::Stop).unwrap();
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
    assert_eq!(error_kind(m.create(c)), ErrorKind::Validation);
    assert!(f.calls.lock().unwrap().is_empty());
    assert_eq!(fs::read_dir(&m.agents).unwrap().count(), 0);
}
#[test]
fn failed_create_removes_plist() {
    let (_d, m, f) = fixture();
    *f.fail_bootstrap.lock().unwrap() = 1;
    assert_eq!(error_kind(m.create(config())), ErrorKind::Command);
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
    let error = m.update("demo", c).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Command);
    assert!(error.message.ends_with("rollback completed"), "{error}");
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
    assert_eq!(error_kind(m.logs("demo")), ErrorKind::Validation);
}
#[test]
fn rejects_symlink_plist() {
    use std::os::unix::fs::symlink;
    let (d, m, _) = fixture();
    let outside = d.path().join("outside");
    fs::write(&outside, "secret").unwrap();
    symlink(outside, m.path("demo").unwrap()).unwrap();
    assert_eq!(error_kind(m.get("demo")), ErrorKind::Validation);
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
    assert_eq!(error_kind(manager.logs("demo")), ErrorKind::Validation);
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
    assert_eq!(
        error_kind(manager.update("demo", config())),
        ErrorKind::Validation
    );
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
        assert_eq!(
            error_kind(m.update("demo", config())),
            ErrorKind::Validation
        );
    }
}
