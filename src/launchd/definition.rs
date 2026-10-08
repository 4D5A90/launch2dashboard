use super::PREFIX;
use crate::domain::{AppError, ErrorKind, ServiceConfig};
use plist::{Dictionary, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
/// launchd output keys and the log file suffix L2D assigns to each.
pub(super) const LOG_FILES: [(&str, &str); 2] =
    [("StandardOutPath", "log"), ("StandardErrorPath", "err.log")];
pub(super) fn log_path(logs_dir: &Path, id: &str, suffix: &str) -> PathBuf {
    logs_dir.join(format!("{id}.{suffix}"))
}
pub(super) fn label(id: &str) -> String {
    format!("{PREFIX}{id}")
}
pub(super) fn to_config(id: &str, d: &Dictionary) -> Result<ServiceConfig, AppError> {
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
/// Writes `c` over `d`, keeping keys L2D does not manage (e.g. ThrottleInterval).
pub(super) fn to_plist(
    c: &ServiceConfig,
    session: &str,
    logs_dir: &Path,
    mut d: Dictionary,
) -> Dictionary {
    d.insert("Label".into(), Value::String(label(&c.id)));
    d.remove("Program");
    d.insert(
        "LimitLoadToSessionType".into(),
        Value::String(session.into()),
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
    for (key, suffix) in LOG_FILES {
        d.insert(
            key.into(),
            Value::String(
                log_path(logs_dir, &c.id, suffix)
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
    }
    d
}
/// Rejects plists whose policy `to_plist` cannot represent, so an edit never drops it silently.
pub(super) fn check_editable(d: &Dictionary) -> Result<(), AppError> {
    let supported_keepalive = match d.get("KeepAlive") {
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
    if d.contains_key("Program") {
        return Err(AppError::new(
            ErrorKind::Validation,
            "Cannot edit a service with a separate Program key",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> ServiceConfig {
        ServiceConfig {
            id: "demo".into(),
            executable: "/bin/echo".into(),
            arguments: vec!["hello <world>".into()],
            working_directory: Some("/tmp".into()),
            environment: [("KEY".into(), "value".into())].into_iter().collect(),
            autostart: true,
            restart_on_failure: true,
        }
    }
    #[test]
    fn config_roundtrips_and_session_is_recorded() {
        let d = to_plist(
            &config(),
            "Background",
            Path::new("/logs"),
            Dictionary::new(),
        );
        assert_eq!(to_config("demo", &d).unwrap(), config());
        assert_eq!(
            d.get("LimitLoadToSessionType").and_then(Value::as_string),
            Some("Background")
        );
        assert_eq!(
            d.get("StandardErrorPath").and_then(Value::as_string),
            Some("/logs/demo.err.log")
        );
    }
    #[test]
    fn unrepresentable_policies_are_not_editable() {
        let ok = to_plist(&config(), "Aqua", Path::new("/logs"), Dictionary::new());
        assert!(check_editable(&ok).is_ok());
        let keepalive_dict = |key: &str, value: bool| {
            Value::Dictionary(
                [(key.to_owned(), Value::Boolean(value))]
                    .into_iter()
                    .collect(),
            )
        };
        for (key, value) in [
            ("KeepAlive", Value::Boolean(true)),
            ("KeepAlive", keepalive_dict("SuccessfulExit", true)),
            ("KeepAlive", keepalive_dict("NetworkState", true)),
            ("Program", Value::String("/bin/sh".into())),
        ] {
            let mut d = ok.clone();
            d.insert(key.into(), value);
            assert_eq!(check_editable(&d).unwrap_err().kind, ErrorKind::Validation);
        }
    }
}
