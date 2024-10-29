use std::{fmt::Debug, mem::replace};

use derivative::Derivative;
use relm4::{gtk::prelude::*, prelude::*};

use crate::greetd::{
    AuthInformative, AuthInformativeResponse, AuthQuestion, AuthQuestionResponse, AuthResponse,
    CancellableSession, CreateSessionResponse, Greetd, StartableSession,
};

/// Initializes the login controls of the greeter.
pub struct GreetdControlsInit<Client>
where
    Client: Greetd + 'static,
{
    /// Initial session state. This way you can present a password prompt immediately on startup.
    pub greetd_state: GreetdState<Client>,

    /// Specifies what username should be used when creating a session.
    pub username: String,

    /// What command to execute when the session is started.
    pub command: Vec<String>,

    /// What env to use when the session is started.
    pub env: Vec<String>,
}

/// Shows greetd session controls.
pub struct GreetdControls<Client>
where
    Client: Greetd + 'static,
{
    /// Stores the last output session+user state
    last_communicated_session_state: SessionState,

    /// Represents what UI is shown to the user.
    greetd_state: GreetdState<Client>,
    /// Username to use when creating a new session.
    username: String,
    /// Command to use when starting a session. This is updated by the parent widget.
    command: Option<Vec<String>>,
    /// Env to use when starting a session.
    env: Vec<String>,

    /// A bool to conditionally reset the question inputs.
    /// Use of tracker::track would not solve the issue because we want to perform a reset only after an authentication
    /// has succeeded or when a session is created.
    reset_question_inputs_event: bool,

    /// An event to perform actions when the page is switched. For example, focus the button/input. Can't `#[watch]`
    /// these calls because the widget receives updates from the outside that may change focus from the origin widget.
    just_switched_screens_event: bool,
}

enum SessionState {
    NotCreated,
    Created,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum GreetdState<Client>
where
    Client: Greetd,
{
    /// In the UI, shows a single login button. When pressed, uses the username stored to create a session.
    NotCreated(#[derivative(Debug = "ignore")] Client),

    /// The session requires no further authentication and can be started. This looks like an info box with a login button.
    Startable(#[derivative(Debug = "ignore")] Client::StartableSession),

    /// An auth prompt (either secret or visible)
    AuthQuestion {
        /// Can be used to retrieve the prompt text and it's type.
        #[derivative(Debug = "ignore")]
        session: Client::AuthQuestion,
    },

    /// An informative auth prompt from greetd. Looks like an info box with the message type set according to what the prompt is - info or error.
    AuthInformative(#[derivative(Debug = "ignore")] Client::AuthInformative),

    /// Used as a placeholder to do 2 things.
    /// 1. Lock the UI while a greetd operation takes place (shows an info box with no buttons).
    /// 2. A temporary value that can be used to move session state out of `&mut self`
    Loading {
        /// Text inside the [`InfoBar`]
        ///
        /// [`InfoBar`]: gtk::InfoBar
        message: String,

        /// How the [`InfoBar`] should be styled.
        ///
        /// [`InfoBar`]: gtk::InfoBar
        message_type: gtk::MessageType,
    },

    /// A value to indicate that a session start request has succeeded and it is time to exit
    SessionStarted,
}

impl<Client> GreetdState<Client>
where
    Client: Greetd,
{
    fn loading(msg: &str) -> Self {
        Self::Loading {
            message: msg.to_string(),
            message_type: gtk::MessageType::Info,
        }
    }

    fn loading_with_error(error: &str) -> Self {
        Self::Loading {
            message: error.to_string(),
            message_type: gtk::MessageType::Error,
        }
    }
}

#[derive(Debug)]
pub enum GreetdControlsOutput {
    /// Tell the parent to show an error that occured during greetd IPC communication.
    NotifyError(String),

    /// Emited to signal that a session for this username has been created and the username cannot be changed without
    /// canceling the current session.
    ///
    /// This output should be handled by locking the user selection input. This widget will ignore all [`UpdateUser`]
    /// input messages until [`SessionCanceledFor`] is emited. Use the username value in this output to display an
    /// accurate username in the UI.
    ///
    /// [`UpdateUser`]: GreetdControlsMsg::UpdateUser
    CreatedSessionFor(String),

    /// The widget is capable of handling user switching again.
    SessionCanceledFor(String),

    /// Emited when the IPC start_session request succeeds.
    SessionStarted,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum GreetdControlsMsg {
    /// External command
    ///
    /// Sent by the user selector to change the user. This sets the username to be used when creating a session. If a user has to be switched,
    /// the current session should be canceled first.
    ///
    /// # Panics
    ///
    /// Sending this request after [`AuthViewOutput::LockUserSelectors`] is sent will cause a panic in the widget. The request is safe to send
    /// after [`AuthViewOutput::UnlockUserSelectors`] is sent. This component can be initialized into a state that prohibits user switching.
    UpdateUser(String),

    /// External command
    ///
    /// Sent by the parent to update the session start params. Has no effect on the UI of this component.
    UpdateSession(Option<Vec<String>>),

    /// Internal message
    ///
    /// Cancels the session
    Cancel,

    /// Internal message
    ///
    /// Advances the authentication to the next step.
    AdvanceAuthentication(
        /// Credential value
        Option<String>,
    ),
}

#[derive(Debug)]
pub enum CommandOutput<Client>
where
    Client: Greetd,
{
    GreetdResponse {
        greetd_state: GreetdState<Client>,
        error: Option<String>,
    },
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for AuthMessageLabel {
    view! {
        gtk::Label {
            set_xalign: -1.0,
        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for LoginBox {
    view! {
        gtk::Box {
            set_halign: gtk::Align::End,
            set_spacing: 15,

        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for CancelButton {
    view! {
        gtk::Button {
            set_label: "Cancel",
        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for LoginButton {
    view! {
        gtk::Button {
            set_focusable: true,
            set_label: "Login",
            set_receives_default: true,
            add_css_class: "suggested-action",
        }
    }
}

#[relm4::component(pub)]
impl<Client> Component for GreetdControls<Client>
where
    Client: Greetd + 'static + Debug,
{
    type Init = GreetdControlsInit<Client>;
    type Input = GreetdControlsMsg;
    type Output = GreetdControlsOutput;
    type CommandOutput = CommandOutput<Client>;

    view! {
        gtk::Box {
            set_spacing: 15,
            set_orientation: gtk::Orientation::Vertical,

            #[name = "auth_conditional"]
            #[transition = "SlideUpDown"]
            match &model.greetd_state {
                GreetdState::Startable(_) => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    append = &gtk::Frame {
                        gtk::InfoBar {
                            set_show_close_button: false,
                            set_message_type: gtk::MessageType::Info,

                            #[template]
                            AuthMessageLabel {
                                set_text: "Session can be started.",
                            }
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                        #[template] LoginButton {
                            #[track( model.just_switched_screens_event )]
                            grab_focus: (),

                            connect_clicked => GreetdControlsMsg::AdvanceAuthentication(None),
                        },
                    }
                }

                GreetdState::AuthQuestion{ session: question } => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    #[template]
                    append = &AuthMessageLabel {
                        #[track( model.just_switched_screens_event )]
                        set_text: question.auth_question().prompt(),
                    },

                    append = match question.auth_question() {
                        AuthQuestion::Secret(prompt) => gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 15,

                            #[name = "password_entry"]
                            gtk::PasswordEntry {
                                #[track( model.just_switched_screens_event )]
                                set_placeholder_text: Some(prompt),

                                #[track( model.reset_question_inputs_event )]
                                set_text: "",

                                set_show_peek_icon: true,

                                #[track( model.just_switched_screens_event )]
                                grab_focus: (),

                                connect_activate[sender] => move |this| {
                                    sender.input(Self::Input::AdvanceAuthentication(Some(this.text().to_string())))
                                }
                            },

                            #[template]
                            append = &LoginBox {
                                #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                                #[template] LoginButton {
                                    connect_clicked[sender, password_entry] => move |_| {
                                        sender.input(GreetdControlsMsg::AdvanceAuthentication(
                                            Some(password_entry.text().to_string())
                                        ))
                                    }
                                },
                            }
                        }

                        AuthQuestion::Visible(prompt) => gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 15,

                            #[name = "visible_entry"]
                            gtk::Entry {
                                #[track( model.just_switched_screens_event )]
                                set_placeholder_text: Some(prompt),

                                #[track( model.reset_question_inputs_event )]
                                set_text: "",

                                #[track( model.just_switched_screens_event )]
                                grab_focus: (),

                                connect_activate[sender] => move |this| {
                                    sender.input(Self::Input::AdvanceAuthentication(Some(this.text().to_string())))
                                }
                            },

                            #[template]
                            append = &LoginBox {
                                #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                                #[template] LoginButton {
                                    connect_clicked[sender, visible_entry] => move |_| {
                                        sender.input(GreetdControlsMsg::AdvanceAuthentication(
                                            Some(visible_entry.text().to_string())
                                        ))
                                    }
                                },
                            }
                        }
                    },
                }

                GreetdState::AuthInformative(informative) => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    // TODO: Refactor this to reuse the infobar and use Revealer on LoginBox
                    append =  &gtk::Box {
                        gtk::Frame {
                            gtk::InfoBar {
                                set_show_close_button: false,

                                #[watch]
                                set_message_type: match informative.auth_informative() {
                                    AuthInformative::Info(_) => gtk::MessageType::Question,
                                    AuthInformative::Error(_) => gtk::MessageType::Error,
                                },

                                #[template]
                                AuthMessageLabel {
                                    set_wrap: true,

                                    #[watch]
                                    set_text: informative.auth_informative().prompt(),
                                },

                            }
                        },

                        #[template]
                        append = &LoginBox {
                            #[template] CancelButton {
                                connect_clicked => GreetdControlsMsg::Cancel
                            },
                            #[template] LoginButton {
                                #[track( model.just_switched_screens_event )]
                                grab_focus: (),

                                connect_clicked => GreetdControlsMsg::AdvanceAuthentication(None),
                            },
                        }
                    },
                }

                GreetdState::Loading{message, message_type} => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    append = &gtk::Frame {
                        gtk::InfoBar {
                            set_show_close_button: false,

                            #[watch]
                            set_message_type: *message_type,

                            #[template]
                            AuthMessageLabel {
                                #[watch]
                                set_text: message.as_str(),
                                set_valign: gtk::Align::Start,
                            }
                        }
                    }
                }

                GreetdState::SessionStarted => gtk::Frame {
                    gtk::InfoBar {
                        set_show_close_button: false,
                        set_message_type: gtk::MessageType::Info,

                        gtk::Separator,

                        #[template]
                        AuthMessageLabel {
                            set_text: "Session started",
                            set_valign: gtk::Align::Start,
                        }
                    }
                }

                GreetdState::NotCreated(_) => gtk::Box {
                    set_halign: gtk::Align::End,
                    #[template] LoginButton {
                        #[track( model.just_switched_screens_event )]
                        grab_focus: (),

                        connect_clicked => GreetdControlsMsg::AdvanceAuthentication(None),
                    },
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let GreetdControlsInit {
            greetd_state,
            username,
            command,
            env,
        } = init;

        let model = Self {
            last_communicated_session_state: if matches!(greetd_state, GreetdState::NotCreated(_)) {
                SessionState::NotCreated
            } else {
                SessionState::Created
            },
            greetd_state,
            username,
            command: Some(command),
            env,

            reset_question_inputs_event: false,
            just_switched_screens_event: true,
        };
        let widgets = view_output!();

        // Note: For some reason in post_view() this didnt work.
        widgets.auth_conditional.set_vhomogeneous(false);
        widgets.auth_conditional.set_interpolate_size(true);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        self.reset_question_inputs_event = false;
        self.just_switched_screens_event = false;

        match message {
            GreetdControlsMsg::Cancel => {
                self.reset_question_inputs_event = true;

                self.cancel_session(&sender)
            }
            GreetdControlsMsg::AdvanceAuthentication(credential) => {
                self.advance_authentication(&sender, credential)
            }

            GreetdControlsMsg::UpdateUser(username) => self.change_user(username),

            GreetdControlsMsg::UpdateSession(command) => self.command = command,
        };

        self.communicate_session_state(&sender);
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.just_switched_screens_event = true;

        let CommandOutput::GreetdResponse {
            greetd_state,
            error,
        } = message;

        if let Some(ref error) = error {
            error!("Greetd error: {error}");
            sender
                .output(GreetdControlsOutput::NotifyError(error.clone()))
                .expect("auth view controller should not be dropped");
        }

        self.greetd_state = match greetd_state {
            GreetdState::Startable(startable) => match &self.command {
                Some(command) => {
                    let env = self.env.clone();
                    let command = command.clone();
                    sender.oneshot_command(async {
                        let (greetd_state, error) =
                            try_start_session(startable, GreetdState::Startable, command, env)
                                .await;

                        CommandOutput::GreetdResponse {
                            greetd_state,
                            error,
                        }
                    });

                    GreetdState::loading("Starting session")
                }

                None => {
                    sender
                        .output(GreetdControlsOutput::NotifyError(
                            "Selected session cannot be executed because it is invalid".to_string(),
                        ))
                        .expect("auth view controller should not be dropped");

                    GreetdState::Startable(startable)
                }
            },

            GreetdState::AuthInformative(informative) => {
                let loading_state = match informative.auth_informative() {
                    AuthInformative::Error(error) => GreetdState::loading_with_error(error),
                    AuthInformative::Info(msg) => GreetdState::loading(msg),
                };

                sender.oneshot_command(async {
                    let (greetd_state, error) =
                        try_auth(informative, GreetdState::AuthInformative, None).await;

                    CommandOutput::GreetdResponse {
                        greetd_state,
                        error,
                    }
                });

                loading_state
            }

            GreetdState::SessionStarted => {
                sender.output(GreetdControlsOutput::SessionStarted).unwrap();

                GreetdState::SessionStarted
            }

            other => other,
        };

        self.communicate_session_state(&sender);

        if matches!(
            (error, &self.greetd_state),
            (None, GreetdState::AuthQuestion { .. })
        ) {
            self.reset_question_inputs_event = true;
        }
    }
}

impl<Client> GreetdControls<Client>
where
    Client: Greetd + 'static + Debug,
{
    fn communicate_session_state(&mut self, sender: &ComponentSender<Self>) {
        // Only communicate session created state once, when the state actually changes.
        match (&self.last_communicated_session_state, &self.greetd_state) {
            (SessionState::Created, GreetdState::NotCreated(_)) => {
                sender
                    .output(GreetdControlsOutput::SessionCanceledFor(
                        self.username.clone(),
                    ))
                    .unwrap();

                self.last_communicated_session_state = SessionState::NotCreated
            }

            (SessionState::NotCreated, created)
                if !matches!(created, GreetdState::NotCreated(_)) =>
            {
                sender
                    .output(GreetdControlsOutput::CreatedSessionFor(
                        self.username.clone(),
                    ))
                    .unwrap();

                self.last_communicated_session_state = SessionState::Created
            }

            _ => (),
        }
    }

    fn cancel_session(&mut self, sender: &ComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(&mut self.greetd_state, S::loading("Canceling session"));

        match greetd_state {
            old @ S::Loading { .. } => self.greetd_state = old,
            S::NotCreated(client) => self.greetd_state = GreetdState::NotCreated(client),

            S::Startable(client) => sender.oneshot_command(async {
                let (greetd_state, error) = try_cancel(client, S::Startable).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::AuthQuestion { session } => sender.oneshot_command(async {
                let (greetd_state, error) =
                    try_cancel(session, move |session| S::AuthQuestion { session }).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::AuthInformative(session) => sender.oneshot_command(async {
                let (greetd_state, error) = try_cancel(session, S::AuthInformative).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::SessionStarted => (),
        };
    }

    fn advance_authentication(
        &mut self,
        sender: &ComponentSender<Self>,
        credential: Option<String>,
    ) {
        use GreetdState as S;

        let greetd_state = replace(&mut self.greetd_state, S::loading("Authenticating"));

        match greetd_state {
            old @ S::Loading { .. } => self.greetd_state = old,
            S::Startable(startable) => match &self.command {
                Some(command) => {
                    let env = self.env.clone();
                    let command = command.clone();
                    sender.oneshot_command(async {
                        let (greetd_state, error) =
                            try_start_session(startable, GreetdState::Startable, command, env)
                                .await;

                        CommandOutput::GreetdResponse {
                            greetd_state,
                            error,
                        }
                    });

                    self.greetd_state = GreetdState::loading("Starting session");
                }

                None => {
                    sender
                        .output(GreetdControlsOutput::NotifyError(
                            "Selected session cannot be executed because it is invalid".to_string(),
                        ))
                        .expect("auth view controller should not be dropped");

                    self.greetd_state = GreetdState::Startable(startable);
                }
            },

            S::NotCreated(client) => {
                let username = self.username.clone();

                sender.oneshot_command(async {
                    let (greetd_state, error) = try_create_session(client, username).await;

                    CommandOutput::GreetdResponse {
                        greetd_state,
                        error,
                    }
                });
            }

            S::AuthQuestion { session } => sender.oneshot_command(async {
                let (greetd_state, error) = try_auth(
                    session,
                    move |session| S::AuthQuestion { session },
                    credential,
                )
                .await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::AuthInformative(informative) => sender.oneshot_command(async {
                let (greetd_state, error) = try_auth(informative, S::AuthInformative, None).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::SessionStarted => (),
        };
    }

    fn change_user(&mut self, username: String) {
        use GreetdState as S;

        match &self.greetd_state {
            S::NotCreated(_) => self.username = username,
            _ => (),
        }
    }
}

async fn try_cancel<Session>(
    session: Session,
    variant: impl FnOnce(Session) -> GreetdState<<Session as CancellableSession>::Client>,
) -> (
    GreetdState<<Session as CancellableSession>::Client>,
    Option<String>,
)
where
    Session: CancellableSession,
{
    debug!("Canceling session");

    let res = match session.cancel_session().await {
        Ok(res) => res,
        Err((session, err)) => return (variant(session), Some(format!("IPC error: {}", err))),
    };

    match res {
        Ok(client) => (GreetdState::NotCreated(client), None),
        Err((session, err)) => (variant(session), Some(format!("Reported error: {}", err))),
    }
}

/// Creates the session but does not start it.
async fn try_create_session<Client>(
    client: Client,
    username: String,
) -> (GreetdState<Client>, Option<String>)
where
    Client: Greetd,
{
    debug!("Creating session for user: {username}");

    let res = match client.create_session(&username).await {
        Ok(res) => res,
        Err((client, err)) => return (GreetdState::NotCreated(client), Some(format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((client, err)) => return (GreetdState::NotCreated(client), Some(format!("{}", err))),
    };

    use CreateSessionResponse as R;
    (
        match session {
            R::Success(startable) => GreetdState::Startable(startable),
            R::AuthQuestion(question) => GreetdState::AuthQuestion { session: question },
            R::AuthInformative(informative) => GreetdState::AuthInformative(informative),
        },
        None,
    )
}

async fn try_start_session<Startable>(
    session: Startable,
    variant: impl FnOnce(Startable) -> GreetdState<<Startable as StartableSession>::Client>,
    command: Vec<String>,
    env: Vec<String>,
) -> (
    GreetdState<<Startable as StartableSession>::Client>,
    Option<String>,
)
where
    Startable: StartableSession,
{
    debug!("Starting session: cmd: {command:?} env: {env:?}");

    let res = match session.start_session(command, env).await {
        Ok(res) => res,
        Err((startable, err)) => return (variant(startable), Some(format!("{}", err))),
    };

    match res {
        Ok(()) => (GreetdState::SessionStarted, None),
        Err((startable, err)) => (variant(startable), Some(format!("{}", err))),
    }
}

async fn try_auth<Message>(
    message: Message,
    variant: impl FnOnce(Message) -> GreetdState<<Message as AuthResponse>::Client>,
    credential: Option<String>,
) -> (
    GreetdState<<Message as AuthResponse>::Client>,
    Option<String>,
)
where
    Message: AuthResponse,
{
    let res = match message.respond(credential).await {
        Ok(res) => res,
        Err((message, err)) => return (variant(message), Some(format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((message, err)) => return (variant(message), Some(format!("{}", err))),
    };

    use CreateSessionResponse as R;
    (
        match session {
            R::Success(startable) => GreetdState::Startable(startable),
            R::AuthQuestion(question) => GreetdState::AuthQuestion { session: question },
            R::AuthInformative(informative) => GreetdState::AuthInformative(informative),
        },
        None,
    )
}
