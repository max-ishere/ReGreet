use std::mem::{replace, take};

use derivative::Derivative;
use gtk4::prelude::*;
use relm4::{
    component::{AsyncComponentParts, SimpleAsyncComponent},
    prelude::*,
    AsyncComponentSender,
};
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
    /// Username to use when creating a new session. When a different username is set by the parent widget, the current session is canceled.
    username: String,
    /// Command to use when starting a session. This is updated by the parent widget.
    command: Option<Vec<String>>,
    /// Env to use when starting a session. This is updated by the parent widget.
    env: Vec<String>,

    /// A bool to conditionally reset the question inputs.
    /// Use of tracker::track would not solve the issue because we want to perform a reset after an authentication has succeeded
    /// or when a session is created.
    reset_question_inputs_event: bool,
}
pub enum GreetdState<Client>
where
    Client: Greetd,
{
    /// In the UI, shows a single login button. When pressed, uses the username stored to create a session.
    NotCreated(Client),

    /// The session requires no further authentication and can be started. This looks like an info box with a login button.
    Startable(Client::StartableSession),

    /// An auth prompt (either secret or visible)
    AuthQuestion {
        /// Can be used to retrieve the prompt text and it's type.
        session: Client::AuthQuestion,
        /// The current value of the input that will be sent to greetd when the login button is pressed.
        credential: String,
    },

    /// An informative auth prompt from greetd. Looks like an info box with the message type set according to what the prompt is - info or error.
    AuthInformative(Client::AuthInformative),

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

#[relm4::component(pub, async)]
impl<Client> SimpleAsyncComponent for GreetdControls<Client>
where
    Client: Greetd + 'static,
{
    type Init = GreetdControlsInit<Client>;
    type Input = GreetdControlsMsg;
    type Output = GreetdControlsOutput;

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

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
                            connect_activate => Self::Input::AdvanceAuthentication,
                        }
                        AuthQuestion::Visible(prompt) => gtk::Entry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            #[track( model.reset_question_inputs_event )]
                            set_text: credential,

                            #[track( model.reset_question_inputs_event )]
                            grab_focus: (),

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
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
                        #[watch]
                        grab_focus: (),

                        connect_clicked => GreetdControlsMsg::AdvanceAuthentication,
                     },
                }

            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
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

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>) {
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
            GreetdControlsMsg::Cancel => self.cancel_session(&sender).await,
            GreetdControlsMsg::AdvanceAuthentication => self.advance_authentication(&sender).await,

            GreetdControlsMsg::UpdateUser(username) => self.change_user(username).await,

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
}

impl<Client> GreetdControls<Client>
where
    Client: Greetd,
{
    pub async fn cancel_session(&mut self, sender: &AsyncComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Canceling session")),
        );

        self.greetd_state = match greetd_state {
            S::Loading(old) => S::Loading(old),
            S::NotCreated(client) => GreetdState::NotCreated(client),

            S::Startable(client) => report_error(try_cancel(client, S::Startable).await, sender),

            S::AuthQuestion {
                session,
                mut credential,
            } => report_error(
                try_cancel(session, |session| S::AuthQuestion {
                    session,
                    credential: take(&mut credential),
                })
                .await,
                sender,
            ),

            S::AuthInformative(session) => {
                report_error(try_cancel(session, S::AuthInformative).await, sender)
            }
        };
    }

    async fn advance_authentication(&mut self, sender: &AsyncComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Canceling session")),
        );

        let maybe_startable = match greetd_state {
            S::Loading(old) => S::Loading(old),

            GreetdState::NotCreated(client) => report_error(
                try_create_session(client, &self.username, || {
                    self.reset_question_inputs_event = true
                })
                .await,
                sender,
            ),
            GreetdState::Startable(startable) => GreetdState::Startable(startable),
            GreetdState::AuthQuestion {
                session,
                credential,
            } => report_error(
                try_auth(
                    session,
                    |session| S::AuthQuestion {
                        session,
                        credential: credential.clone(),
                    },
                    Some(credential.clone()),
                    || self.reset_question_inputs_event = true,
                )
                .await,
                sender,
            ),
            GreetdState::AuthInformative(informative) => report_error(
                try_auth(informative, S::AuthInformative, None, || {
                    self.reset_question_inputs_event = true
                })
                .await,
                sender,
            ),
        };

        self.greetd_state = try_autostart(
            maybe_startable,
            self.command.clone(),
            self.env.clone(),
            sender,
        )
        .await;
    }

    async fn change_user(&mut self, username: String) {
        use GreetdState as S;

        match &self.greetd_state {
            S::NotCreated(_) => self.username = username,
            _user_cannot_be_switched_infallibly => {
                panic!("The user cannot be switched in this Greetd IPC state.")
            }
        }
    }
}

fn report_error<Client>(
    res: Result<GreetdState<Client>, (GreetdState<Client>, String)>,
    sender: &AsyncComponentSender<GreetdControls<Client>>,
) -> GreetdState<Client>
where
    Client: Greetd,
{
    match res {
        Ok(state) => state,
        Err((state, err)) => {
            error!("Greetd error: {err}");
            sender
                .output(GreetdControlsOutput::NotifyError(err))
                .expect("auth view controller should not be dropped");
            state
        }
    }
}

async fn try_cancel<Session>(
    session: Session,
    variant: impl FnOnce(Session) -> GreetdState<<Session as CancellableSession>::Client>,
) -> Result<
    GreetdState<<Session as CancellableSession>::Client>,
    (GreetdState<<Session as CancellableSession>::Client>, String),
>
where
    Session: CancellableSession,
{
    debug!("Canceling session");

    let res = match session.cancel_session().await {
        Ok(res) => res,
        Err((session, err)) => return Err((variant(session), format!("{}", err))),
    };

    match res {
        Ok(client) => Ok(GreetdState::NotCreated(client)),
        Err((session, err)) => Err((variant(session), format!("{}", err))),
    }
}

/// Creates the session but does not start it.
async fn try_create_session<Client>(
    client: Client,
    username: &str,
    on_auth: impl FnOnce(),
) -> Result<GreetdState<Client>, (GreetdState<Client>, String)>
where
    Client: Greetd,
{
    debug!("Creating session for user: {username}");

    let res = match client.create_session(username).await {
        Ok(res) => res,
        Err((client, err)) => return Err((GreetdState::NotCreated(client), format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((client, err)) => return Err((GreetdState::NotCreated(client), format!("{}", err))),
    };

    use CreateSessionResponse as R;
    Ok(match session {
        R::Success(startable) => GreetdState::Startable(startable),
        R::AuthQuestion(question) => {
            on_auth();
            GreetdState::AuthQuestion {
                session: question,
                credential: String::new(),
            }
        }
        R::AuthInformative(informative) => GreetdState::AuthInformative(informative),
    })
}

async fn try_start_session<Startable>(
    session: Startable,
    variant: impl FnOnce(Startable) -> GreetdState<<Startable as StartableSession>::Client>,
    command: Vec<String>,
    env: Vec<String>,
) -> Result<
    GreetdState<<Startable as StartableSession>::Client>,
    (GreetdState<<Startable as StartableSession>::Client>, String),
>
where
    Startable: StartableSession,
{
    debug!("Starting session: cmd: {command:?} env: {env:?}");

    let res = match session.start_session(command, env).await {
        Ok(res) => res,
        Err((startable, err)) => return Err((variant(startable), format!("{}", err))),
    };

    match res {
        Ok(client) => Ok(GreetdState::NotCreated(client)),
        Err((startable, err)) => Err((variant(startable), format!("{}", err))),
    }
}

async fn try_autostart<Client>(
    state: GreetdState<Client>,
    command: Option<Vec<String>>,
    env: Vec<String>,
    sender: &AsyncComponentSender<GreetdControls<Client>>,
) -> GreetdState<Client>
where
    Client: Greetd,
{
    if let GreetdState::Startable(startable) = state {
        let Some(command) = command else {
            sender
                .output(GreetdControlsOutput::NotifyError(
                    "Selected session cannot be executed because it is invalid".to_string(),
                ))
                .expect("auth view controller should not be dropped");

            return GreetdState::Startable(startable);
        };

        report_error(
            try_start_session(startable, GreetdState::<Client>::Startable, command, env).await,
            sender,
        )
    } else {
        state
    }
}

async fn try_auth<Message>(
    message: Message,
    variant: impl FnOnce(Message) -> GreetdState<<Message as AuthResponse>::Client>,
    credential: Option<String>,
    on_success: impl FnOnce(),
) -> Result<
    GreetdState<<Message as AuthResponse>::Client>,
    (GreetdState<<Message as AuthResponse>::Client>, String),
>
where
    Message: AuthResponse,
{
    let res = match message.respond(credential).await {
        Ok(res) => res,
        Err((message, err)) => return Err((variant(message), format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((message, err)) => return Err((variant(message), format!("{}", err))),
    };

    on_success();

    use CreateSessionResponse as R;
    Ok(match session {
        R::Success(startable) => GreetdState::Startable(startable),
        R::AuthQuestion(question) => GreetdState::AuthQuestion {
            session: question,
            credential: String::new(),
        },
        R::AuthInformative(informative) => GreetdState::AuthInformative(informative),
    })
}
