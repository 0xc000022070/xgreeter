use crate::greetd::{AuthMessageType, ErrorType, Request, Response};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    User,
    Password,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Creating,
    /// greetd asked something not pre-filled; caret is live in the field.
    Prompt { secret: bool, message: String },
    Authenticating,
    Starting,
    Done,
    Failed(String),
}

#[derive(Debug)]
pub enum Effect {
    Send(Request),
    /// Restore the terminal and exit 0 so greetd execs the session.
    LaunchAndExit,
    Quit,
}

// greetd_ipc::Request has no PartialEq; compare structurally so tests can
// assert on emitted effects.
impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Effect::LaunchAndExit, Effect::LaunchAndExit) => true,
            (Effect::Quit, Effect::Quit) => true,
            (Effect::Send(a), Effect::Send(b)) => request_eq(a, b),
            _ => false,
        }
    }
}

fn request_eq(a: &Request, b: &Request) -> bool {
    match (a, b) {
        (Request::CreateSession { username: x }, Request::CreateSession { username: y }) => x == y,
        (
            Request::PostAuthMessageResponse { response: x },
            Request::PostAuthMessageResponse { response: y },
        ) => x == y,
        (
            Request::StartSession { cmd: c1, env: e1 },
            Request::StartSession { cmd: c2, env: e2 },
        ) => c1 == c2 && e1 == e2,
        (Request::CancelSession, Request::CancelSession) => true,
        _ => false,
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub brand: String,
    pub user: String,
    pub password: String,
    pub focus: Field,
    pub phase: Phase,
    pub info: Option<String>,
    pub demo: bool,
    pub tick: u64,
    session_cmd: Vec<String>,
    /// The typed password still needs to be auto-fed to the first secret prompt.
    creds_pending: bool,
}

#[derive(Debug)]
pub enum Action {
    Char(char),
    Backspace,
    FocusToggle,
    Submit,
    Cancel,
    Greetd(Response),
    Tick,
}

impl AppState {
    pub fn new(brand: String, user: String, session_cmd: Vec<String>, demo: bool) -> Self {
        let focus = if user.is_empty() {
            Field::User
        } else {
            Field::Password
        };
        AppState {
            brand,
            user,
            password: String::new(),
            focus,
            phase: Phase::Idle,
            info: None,
            demo,
            tick: 0,
            session_cmd,
            creds_pending: false,
        }
    }

    fn in_auth_chain(&self) -> bool {
        matches!(
            self.phase,
            Phase::Creating | Phase::Prompt { .. } | Phase::Authenticating | Phase::Starting
        )
    }

    pub fn editable(&self) -> bool {
        matches!(self.phase, Phase::Idle | Phase::Failed(_))
            || matches!(self.phase, Phase::Prompt { .. })
    }

    fn active_buffer(&mut self) -> &mut String {
        // During a prompt all typing goes to the password buffer regardless of
        // focus; the prompt's secret flag decides how it renders.
        match self.phase {
            Phase::Prompt { .. } => &mut self.password,
            _ => match self.focus {
                Field::User => &mut self.user,
                Field::Password => &mut self.password,
            },
        }
    }

    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Tick => {
                self.tick = self.tick.wrapping_add(1);
                Vec::new()
            }
            Action::Char(c) if self.editable() && !c.is_control() => {
                if matches!(self.phase, Phase::Failed(_)) {
                    self.phase = Phase::Idle;
                }
                self.active_buffer().push(c);
                Vec::new()
            }
            Action::Char(_) => Vec::new(),
            Action::Backspace if self.editable() => {
                self.active_buffer().pop();
                Vec::new()
            }
            Action::Backspace => Vec::new(),
            Action::FocusToggle => {
                if matches!(self.phase, Phase::Idle | Phase::Failed(_)) {
                    self.focus = match self.focus {
                        Field::User => Field::Password,
                        Field::Password => Field::User,
                    };
                }
                Vec::new()
            }
            Action::Submit => self.on_submit(),
            Action::Cancel => self.on_cancel(),
            Action::Greetd(resp) => self.on_greetd(resp),
        }
    }

    fn on_submit(&mut self) -> Vec<Effect> {
        match &self.phase {
            Phase::Idle | Phase::Failed(_) => {
                if self.user.trim().is_empty() {
                    return Vec::new();
                }
                self.info = None;
                self.creds_pending = true;
                self.phase = Phase::Creating;
                vec![Effect::Send(Request::CreateSession {
                    username: self.user.clone(),
                })]
            }
            Phase::Prompt { .. } => {
                let response = Some(std::mem::take(&mut self.password));
                self.phase = Phase::Authenticating;
                vec![Effect::Send(Request::PostAuthMessageResponse { response })]
            }
            _ => Vec::new(),
        }
    }

    fn on_cancel(&mut self) -> Vec<Effect> {
        if self.in_auth_chain() {
            self.reset_to_idle();
            return vec![Effect::Send(Request::CancelSession)];
        }
        // Only demo may leave; the real greeter must never exit without a session.
        if self.demo {
            vec![Effect::Quit]
        } else {
            Vec::new()
        }
    }

    fn reset_to_idle(&mut self) {
        self.phase = Phase::Idle;
        self.password.clear();
        self.creds_pending = false;
        self.focus = Field::Password;
    }

    fn on_greetd(&mut self, resp: Response) -> Vec<Effect> {
        match resp {
            Response::AuthMessage {
                auth_message_type,
                auth_message,
            } => self.on_auth_message(auth_message_type, auth_message),
            Response::Success => self.on_success(),
            Response::Error {
                error_type,
                description,
            } => self.on_error(error_type, description),
        }
    }

    fn on_auth_message(&mut self, kind: AuthMessageType, message: String) -> Vec<Effect> {
        if !self.in_auth_chain() {
            return Vec::new();
        }
        match kind {
            AuthMessageType::Secret | AuthMessageType::Visible => {
                let secret = matches!(kind, AuthMessageType::Secret);
                // Auto-feed the pre-typed password to the first secret prompt.
                if secret && self.creds_pending {
                    self.creds_pending = false;
                    let response = Some(self.password.clone());
                    self.phase = Phase::Authenticating;
                    vec![Effect::Send(Request::PostAuthMessageResponse { response })]
                } else {
                    self.password.clear();
                    self.focus = Field::Password;
                    self.phase = Phase::Prompt { secret, message };
                    Vec::new()
                }
            }
            AuthMessageType::Info | AuthMessageType::Error => {
                // Not a prompt; acknowledge with an empty response to advance.
                self.info = Some(message);
                vec![Effect::Send(Request::PostAuthMessageResponse { response: None })]
            }
        }
    }

    fn on_success(&mut self) -> Vec<Effect> {
        match self.phase {
            Phase::Starting => {
                self.phase = Phase::Done;
                vec![Effect::LaunchAndExit]
            }
            _ if self.in_auth_chain() => {
                self.phase = Phase::Starting;
                self.info = None;
                vec![Effect::Send(Request::StartSession {
                    cmd: self.session_cmd.clone(),
                    env: Vec::new(),
                })]
            }
            _ => Vec::new(),
        }
    }

    fn on_error(&mut self, kind: ErrorType, description: String) -> Vec<Effect> {
        self.creds_pending = false;
        self.password.clear();
        self.focus = Field::Password;
        let msg = match kind {
            ErrorType::AuthError => {
                if description.is_empty() {
                    "access denied".into()
                } else {
                    description
                }
            }
            ErrorType::Error => format!("error: {description}"),
        };
        self.phase = Phase::Failed(msg);
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> AppState {
        AppState::new(
            "0xc000022070".into(),
            "0xc000022070".into(),
            vec!["start-hyprland".into()],
            true,
        )
    }

    #[test]
    fn happy_path_reaches_launch() {
        let mut a = app();
        a.password = "hunter2".into();

        let e = a.update(Action::Submit);
        assert_eq!(
            e,
            vec![Effect::Send(Request::CreateSession {
                username: "0xc000022070".into()
            })]
        );
        assert_eq!(a.phase, Phase::Creating);

        // greetd asks for the password; the pre-typed one is auto-fed.
        let e = a.update(Action::Greetd(Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password: ".into(),
        }));
        assert_eq!(
            e,
            vec![Effect::Send(Request::PostAuthMessageResponse {
                response: Some("hunter2".into())
            })]
        );
        assert_eq!(a.phase, Phase::Authenticating);

        // Auth accepted -> start the session.
        let e = a.update(Action::Greetd(Response::Success));
        assert_eq!(
            e,
            vec![Effect::Send(Request::StartSession {
                cmd: vec!["start-hyprland".into()],
                env: vec![]
            })]
        );
        assert_eq!(a.phase, Phase::Starting);

        // Session started -> hand off.
        let e = a.update(Action::Greetd(Response::Success));
        assert_eq!(e, vec![Effect::LaunchAndExit]);
        assert_eq!(a.phase, Phase::Done);
    }

    #[test]
    fn wrong_password_lands_in_failed_and_clears_secret() {
        let mut a = app();
        a.password = "bad".into();
        a.update(Action::Submit);
        a.update(Action::Greetd(Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password: ".into(),
        }));
        let e = a.update(Action::Greetd(Response::Error {
            error_type: ErrorType::AuthError,
            description: String::new(),
        }));
        assert!(e.is_empty());
        assert_eq!(a.phase, Phase::Failed("access denied".into()));
        assert_eq!(a.password, "");
    }

    #[test]
    fn retry_after_failure_creates_fresh_session() {
        let mut a = app();
        a.phase = Phase::Failed("access denied".into());
        a.user = "0xc000022070".into();
        let e = a.update(Action::Submit);
        assert_eq!(
            e,
            vec![Effect::Send(Request::CreateSession {
                username: "0xc000022070".into()
            })]
        );
    }

    #[test]
    fn typing_clears_prior_failure() {
        let mut a = app();
        a.phase = Phase::Failed("access denied".into());
        a.focus = Field::Password;
        a.update(Action::Char('x'));
        assert_eq!(a.phase, Phase::Idle);
        assert_eq!(a.password, "x");
    }

    #[test]
    fn no_submit_without_username() {
        let mut a = AppState::new("b".into(), String::new(), vec!["s".into()], true);
        assert!(a.update(Action::Submit).is_empty());
        assert_eq!(a.phase, Phase::Idle);
    }

    #[test]
    fn cancel_midflight_sends_cancel_and_resets() {
        let mut a = app();
        a.update(Action::Submit);
        let e = a.update(Action::Cancel);
        assert_eq!(e, vec![Effect::Send(Request::CancelSession)]);
        assert_eq!(a.phase, Phase::Idle);
    }

    #[test]
    fn stray_response_while_idle_is_ignored() {
        let mut a = app();
        assert!(a.update(Action::Greetd(Response::Success)).is_empty());
        assert_eq!(a.phase, Phase::Idle);
    }

    #[test]
    fn info_message_is_acknowledged_without_leaving_chain() {
        let mut a = app();
        a.update(Action::Submit);
        let e = a.update(Action::Greetd(Response::AuthMessage {
            auth_message_type: AuthMessageType::Info,
            auth_message: "insert smartcard".into(),
        }));
        assert_eq!(
            e,
            vec![Effect::Send(Request::PostAuthMessageResponse { response: None })]
        );
        assert_eq!(a.info.as_deref(), Some("insert smartcard"));
    }

    #[test]
    fn demo_cancel_at_idle_quits_real_does_not() {
        let mut demo = app();
        assert_eq!(demo.update(Action::Cancel), vec![Effect::Quit]);

        let mut real = AppState::new("b".into(), "u".into(), vec!["s".into()], false);
        assert!(real.update(Action::Cancel).is_empty());
    }
}
