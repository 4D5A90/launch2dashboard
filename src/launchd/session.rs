use super::executor::Executor;
use crate::domain::{AppError, ErrorKind};
#[derive(Clone, Copy)]
pub(super) enum LaunchDomain {
    Gui(u32),
    User(u32),
}
impl LaunchDomain {
    pub(super) fn target(self) -> String {
        match self {
            Self::Gui(uid) => format!("gui/{uid}"),
            Self::User(uid) => format!("user/{uid}"),
        }
    }
    pub(super) fn session(self) -> &'static str {
        match self {
            Self::Gui(_) => "Aqua",
            Self::User(_) => "Background",
        }
    }
    pub(super) fn detect(uid: u32, executor: &dyn Executor) -> Result<Self, AppError> {
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

#[cfg(test)]
mod tests {
    use super::super::testing::{response, scripted};
    use super::*;
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
    fn ssh_without_gui_uses_user_domain() {
        let executor = scripted(vec![
            (&["print", "gui/501"], response(125, "")),
            (&["print", "user/501"], response(0, "")),
        ]);
        let domain = LaunchDomain::detect(501, executor.as_ref()).unwrap();
        assert_eq!(domain.target(), "user/501");
        assert_eq!(domain.session(), "Background");
    }
}
