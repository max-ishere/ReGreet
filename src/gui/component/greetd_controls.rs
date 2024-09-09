use std::{
    fmt::Debug,
    mem::{replace, take},
};

use derivative::Derivative;
use gtk4::prelude::*;
use relm4::prelude::*;
use tracing::{debug, error};

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
    /// This is stored in the model and when the username changes the previous session is canceled.
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
    /// Represents what UI is shown to the user.
    greetd_state: GreetdState<Client>,
    /// Username to use when creating a new session. When a different username is set by the parent widget, the current
    /// session is canceled.
    username: String,
    /// Command to use when starting a session. This is updated by the parent widget.
    command: Option<Vec<String>>,
    /// Env to use when starting a session. This is updated by the parent widget.
    env: Vec<String>,

    /// A bool to conditionally reset the question inputs.
    /// Use of tracker::track would not solve the issue because we want to perform a reset only after an authentication
    /// has succeeded or when a session is created.
    reset_question_inputs_event: bool,
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
        /// The current value of the input that will be sent to greetd when the login button is pressed.
        credential: String,
    },

    /// An informative auth prompt from greetd. Looks like an info box with the message type set according to what the prompt is - info or error.
    AuthInformative(#[derivative(Debug = "ignore")] Client::AuthInformative),

    /// Used as a placeholder to do 2 things.
    /// 1. Lock the UI while a greetd operation takes place (shows an info box with no buttons).
    /// 2. A temporary value that can be used to move session state out of `&mut self`
    Loading(
        /// Message shown while an operation is pending. This will always be an info message type.
        String,
    ),
}

#[derive(Debug)]
pub enum GreetdControlsOutput {
    /// Tell the parent to show an error that occured during greetd IPC communication.
    NotifyError(String),

    /// Emited whenever this UI demands it recieves no user switching requests because a user switch cannot be guaranteed
    /// to be successful (a divertion of the user displayed in the UI and in the created session is forbidden). If the
    /// user switch fails it may leave the UI in an inconsistent state (mitigated by a panic).
    ///
    /// # Panics
    ///
    /// To avoid panics, ensure all user switching UI is locked.
    LockUserSelectors,

    /// The widget is capable of handling user switching again.
    UnlockUserSelectors,
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
    /// Emited by the question auth inputs to update the response.
    CredentialChanged(#[derivative(Debug = "ignore")] String),

    /// Internal message
    ///
    /// Cancels the session
    Cancel,

    /// Internal message
    ///
    /// Advances the authentication to the next step.
    AdvanceAuthentication,
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

                    append = &gtk::InfoBar {
                        set_show_close_button: false,
                        set_message_type: gtk::MessageType::Info,

                        #[template]
                        AuthMessageLabel {
                            set_text: "Session can be started.",
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                        #[template] LoginButton {
                            #[watch]
                            grab_focus: (),

                            connect_clicked => GreetdControlsMsg::AdvanceAuthentication,
                        },
                    }
                }

                GreetdState::AuthQuestion{ session: question, credential } => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    #[template]
                    append = &AuthMessageLabel {
                        #[watch]
                        set_text: question.auth_question().prompt(),
                    },

                    append = match question.auth_question() {
                        AuthQuestion::Secret(prompt) => gtk::PasswordEntry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            #[track( model.reset_question_inputs_event )]
                            set_text: credential,

                            set_show_peek_icon: true,

                            #[track( model.reset_question_inputs_event )]
                            grab_focus: (),

                            connect_changed[sender] => move |this| {
                                sender.input(Self::Input::CredentialChanged(this.text().to_string()))
                            },
                            connect_activate => Self::Input::AdvanceAuthentication,
                        }
                        AuthQuestion::Visible(prompt) => gtk::Entry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            #[track( model.reset_question_inputs_event )]
                            set_text: credential,

                            #[track( model.reset_question_inputs_event )]
                            grab_focus: (),

                            connect_changed[sender] => move |this| {
                                sender.input(Self::Input::CredentialChanged(this.text().to_string()))
                            },
                            connect_activate => Self::Input::AdvanceAuthentication,
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                        #[template] LoginButton { connect_clicked => GreetdControlsMsg::AdvanceAuthentication },
                    }
                }

                GreetdState::AuthInformative(informative) => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator,

                    append = &gtk::InfoBar {
                        set_show_close_button: false,

                        #[watch]
                        set_message_type: match informative.auth_informative() {
                            AuthInformative::Info(_) => gtk::MessageType::Question,
                            AuthInformative::Error(_) => gtk::MessageType::Error,
                        },

                    #[template]
                    AuthMessageLabel {
                            #[watch]
                            set_text: informative.auth_informative().prompt(),
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => GreetdControlsMsg::Cancel },
                        #[template] LoginButton {
                            #[watch]
                            grab_focus: (),

                            connect_clicked => GreetdControlsMsg::AdvanceAuthentication,
                        },
                    }
                }

                GreetdState::Loading(message) => gtk::InfoBar {
                    set_show_close_button: false,
                    set_message_type: gtk::MessageType::Info,

                    gtk::Separator,

                    #[template]
                    AuthMessageLabel {
                        #[watch]
                        set_text: message.as_str(),
                        set_valign: gtk::Align::Start,
                    }
                }

                GreetdState::NotCreated(_) => gtk::Box {
                    set_halign: gtk::Align::End,
                    #[template] LoginButton {
                        grab_focus: (),

                        connect_clicked => GreetdControlsMsg::AdvanceAuthentication,
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

        let reset_question_inputs_event = matches!(greetd_state, GreetdState::AuthQuestion { .. });

        let model = Self {
            greetd_state,
            username,
            command: Some(command),
            env,

            reset_question_inputs_event,
        };
        let widgets = view_output!();

        // Note: For some reason in post_view() this didnt work.
        widgets.auth_conditional.set_vhomogeneous(false);
        widgets.auth_conditional.set_interpolate_size(true);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        self.reset_question_inputs_event = false;

        match message {
            GreetdControlsMsg::CredentialChanged(new_cred) => {
                if let GreetdState::AuthQuestion {
                    ref mut credential, ..
                } = self.greetd_state
                {
                    *credential = new_cred;
                }
            }
            GreetdControlsMsg::Cancel => self.cancel_session(&sender),
            GreetdControlsMsg::AdvanceAuthentication => self.advance_authentication(&sender),

            GreetdControlsMsg::UpdateUser(username) => self.change_user(username),

            GreetdControlsMsg::UpdateSession(command) => {
                self.command = command;
            }
        };

        if let GreetdState::NotCreated(_) = self.greetd_state {
            sender
                .output(GreetdControlsOutput::UnlockUserSelectors)
                .unwrap();
        } else {
            sender
                .output(GreetdControlsOutput::LockUserSelectors)
                .unwrap();
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        let CommandOutput::GreetdResponse {
            greetd_state,
            error,
        } = message;

        if let Some(error) = error {
            error!("Greetd error: {error}");
            sender
                .output(GreetdControlsOutput::NotifyError(error))
                .expect("auth view controller should not be dropped");
        }

        self.greetd_state = if let GreetdState::Startable(startable) = greetd_state {
            match &self.command {
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

                    GreetdState::Loading("Starting session".to_string())
                }

                None => {
                    sender
                        .output(GreetdControlsOutput::NotifyError(
                            "Selected session cannot be executed because it is invalid".to_string(),
                        ))
                        .expect("auth view controller should not be dropped");

                    GreetdState::Startable(startable)
                }
            }
        } else {
            greetd_state
        };

        if let GreetdState::NotCreated(_) = self.greetd_state {
            sender
                .output(GreetdControlsOutput::UnlockUserSelectors)
                .unwrap();
        } else {
            sender
                .output(GreetdControlsOutput::LockUserSelectors)
                .unwrap();
        }
    }
}

impl<Client> GreetdControls<Client>
where
    Client: Greetd + 'static + Debug,
{
    pub fn cancel_session(&mut self, sender: &ComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Canceling session")),
        );

        match greetd_state {
            S::Loading(old) => self.greetd_state = S::Loading(old),
            S::NotCreated(client) => self.greetd_state = GreetdState::NotCreated(client),

            S::Startable(client) => sender.oneshot_command(async {
                let (greetd_state, error) = try_cancel(client, S::Startable).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            S::AuthQuestion {
                session,
                mut credential,
            } => sender.oneshot_command(async {
                let (greetd_state, error) = try_cancel(session, move |session| S::AuthQuestion {
                    session,
                    credential: take(&mut credential),
                })
                .await;

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
        };
    }

    fn advance_authentication(&mut self, sender: &ComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Authenticating")),
        );

        match greetd_state {
            S::Loading(old) => self.greetd_state = S::Loading(old),
            GreetdState::Startable(startable) => {
                self.greetd_state = GreetdState::Startable(startable)
            }

            GreetdState::NotCreated(client) => {
                let username = self.username.clone();

                sender.oneshot_command(async {
                    let (greetd_state, error) = try_create_session(client, username).await;

                    CommandOutput::GreetdResponse {
                        greetd_state,
                        error,
                    }
                });
            }

            GreetdState::AuthQuestion {
                session,
                mut credential,
            } => sender.oneshot_command(async {
                let cred = Some(credential.clone());

                let (greetd_state, error) = try_auth(
                    session,
                    move |session| S::AuthQuestion {
                        session,
                        credential: take(&mut credential),
                    },
                    cred,
                )
                .await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),

            GreetdState::AuthInformative(informative) => sender.oneshot_command(async {
                let (greetd_state, error) = try_auth(informative, S::AuthInformative, None).await;

                CommandOutput::GreetdResponse {
                    greetd_state,
                    error,
                }
            }),
        };
    }

    fn change_user(&mut self, username: String) {
        use GreetdState as S;

        match &self.greetd_state {
            S::NotCreated(_) => self.username = username,
            _user_cannot_be_switched_infallibly => {
                unreachable!("The user cannot be switched in this Greetd IPC state without a chance of it failing. Please ensure the controls are locked.")
            }
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
        Err((session, err)) => return (variant(session), Some(format!("Reported error: {}", err))),
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
            R::AuthQuestion(question) => GreetdState::AuthQuestion {
                session: question,
                credential: String::new(),
            },
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
        Ok(client) => (GreetdState::NotCreated(client), None),
        Err((startable, err)) => return (variant(startable), Some(format!("{}", err))),
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
    let res = match message.respond(dbg!(credential)).await {
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
            R::AuthQuestion(question) => GreetdState::AuthQuestion {
                session: question,
                credential: String::new(),
            },
            // TODO: For info, mimic what https://github.com/rharish101/ReGreet/pull/4 does.
            R::AuthInformative(informative) => GreetdState::AuthInformative(informative),
        },
        None,
    )
}