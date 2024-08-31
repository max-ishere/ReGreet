use std::mem::{replace, take};

use derivative::Derivative;
use gtk4::prelude::*;
use relm4::component::{AsyncComponentParts, SimpleAsyncComponent};
use relm4::{prelude::*, AsyncComponentSender};
use tracing::error;

use crate::greetd::{
    AuthInformative, AuthInformativeResponse, AuthQuestion, AuthQuestionResponse, AuthResponse,
    CancellableSession, CreateSessionResponse, Greetd, StartableSession,
};
use crate::gui::templates::LoginButton;

/// Initializes the login controls of the greeter.
pub struct AuthViewInit<Client>
where
    Client: Greetd + 'static,
{
    /// Initial session state. This way you can present a password prompt immediately on startup.
    pub greetd_state: GreetdState<Client>,
    /// Specifies what username should be used when creating a session.
    /// This is stored in the model and when the username changes the previous session is canceled.
    // TODO: Cancel the session when the username is changed.
    pub username: String,
    /// What command to execute when the session is started.
    pub command: Vec<String>,
    /// What env to use when the session is started.
    pub env: Vec<String>,
}

/// Shows greetd session controls.
pub struct AuthView<Client>
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
    reset_question: bool,
}
pub enum GreetdState<Client>
where
    Client: Greetd,
{
    /// In the UI, shows a single login button. When pressed, uses the username stored to start a session.
    NotStarted(Client),

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
pub enum AuthViewOutput {
    /// Tell the parent to show an error that occured during greetd IPC communication.
    NotifyError(String),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum AuthViewMsg {
    /// External command
    ///
    /// Sent by the parent to update the session start params. Has no effect on the UI of this component.
    UpdateSession { command: Option<Vec<String>> },

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

#[relm4::component(pub, async)]
impl<Client> SimpleAsyncComponent for AuthView<Client>
where
    Client: Greetd + 'static,
{
    type Init = AuthViewInit<Client>;
    type Input = AuthViewMsg;
    type Output = AuthViewOutput;

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

                    gtk::InfoBar {
                        set_show_close_button: false,
                        set_message_type: gtk::MessageType::Info,

                        gtk::Label {
                            set_text: "Click login to start the session.",
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => AuthViewMsg::Cancel },
                        #[template] LoginButton { connect_clicked => AuthViewMsg::AdvanceAuthentication },
                    }
                }

                GreetdState::AuthQuestion{ session: question, .. } => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Label {
                        #[watch]
                        set_text: question.auth_question().prompt(),
                    },

                    append = match question.auth_question() {
                        AuthQuestion::Secret(prompt) => gtk::PasswordEntry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            #[track( model.reset_question )]
                            set_text: "",

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
                        }
                        AuthQuestion::Visible(prompt) => gtk::Entry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            #[track( model.reset_question )]
                            set_text: "",

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => AuthViewMsg::Cancel },
                        #[template] LoginButton { connect_clicked => AuthViewMsg::AdvanceAuthentication },
                    }
                }

                GreetdState::AuthInformative(informative) => gtk::Box {
                    set_spacing: 15,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::InfoBar {
                        set_show_close_button: false,

                        #[watch]
                        set_message_type: match informative.auth_informative() {
                            AuthInformative::Info(_) => gtk::MessageType::Question,
                            AuthInformative::Error(_) => gtk::MessageType::Error,
                        },

                        gtk::Label {
                            #[watch]
                            set_text: informative.auth_informative().prompt(),
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => AuthViewMsg::Cancel },
                        #[template] LoginButton { connect_clicked => AuthViewMsg::AdvanceAuthentication },
                    }
                }

                GreetdState::NotStarted(_) => gtk::Box {
                    set_halign: gtk::Align::End,
                    #[template] LoginButton { connect_clicked => AuthViewMsg::AdvanceAuthentication },
                }

                GreetdState::Loading(message) => gtk::InfoBar {
                    set_show_close_button: false,
                    set_message_type: gtk::MessageType::Info,

                    gtk::Label {
                        #[watch]
                        set_text: message.as_str(),
                        set_valign: gtk::Align::Start,
                    }
                }
            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let AuthViewInit {
            greetd_state,
            username,
            command,
            env,
        } = init;

        let model = Self {
            greetd_state,
            username,
            command: Some(command),
            env,

            reset_question: false,
        };
        let widgets = view_output!();

        // Note: For some reason in post_view() this didnt work.
        widgets.auth_conditional.set_vhomogeneous(false);
        widgets.auth_conditional.set_interpolate_size(true);

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>) {
        self.reset_question = false;

        match message {
            AuthViewMsg::CredentialChanged(new_cred) => {
                if let GreetdState::AuthQuestion {
                    ref mut credential, ..
                } = self.greetd_state
                {
                    *credential = new_cred;
                }
            }
            AuthViewMsg::Cancel => self.cancel_session(&sender).await,
            AuthViewMsg::AdvanceAuthentication => self.advance_authentication(&sender).await,

            AuthViewMsg::UpdateSession { command } => {
                self.command = command;
            }
        };
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

impl<Client> AuthView<Client>
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
            S::NotStarted(client) => GreetdState::NotStarted(client),

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

    pub async fn advance_authentication(&mut self, sender: &AsyncComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Canceling session")),
        );

        let maybe_startable = match greetd_state {
            S::Loading(old) => S::Loading(old),

            GreetdState::NotStarted(client) => report_error(
                try_create_session(client, &self.username, || self.reset_question = true).await,
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
                    || self.reset_question = true,
                )
                .await,
                sender,
            ),
            GreetdState::AuthInformative(informative) => report_error(
                try_auth(informative, S::AuthInformative, None, || {
                    self.reset_question = true
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
}

fn report_error<Client>(
    res: Result<GreetdState<Client>, (GreetdState<Client>, String)>,
    sender: &AsyncComponentSender<AuthView<Client>>,
) -> GreetdState<Client>
where
    Client: Greetd,
{
    match res {
        Ok(state) => state,
        Err((state, err)) => {
            error!("Greetd error: {err}");
            sender
                .output(AuthViewOutput::NotifyError(err))
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
    let res = match session.cancel_session().await {
        Ok(res) => res,
        Err((session, err)) => return Err((variant(session), format!("{}", err))),
    };

    match res {
        Ok(client) => Ok(GreetdState::NotStarted(client)),
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
    let res = match client.create_session(username).await {
        Ok(res) => res,
        Err((client, err)) => return Err((GreetdState::NotStarted(client), format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((client, err)) => return Err((GreetdState::NotStarted(client), format!("{}", err))),
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
    let res = match session.start_session(command, env).await {
        Ok(res) => res,
        Err((startable, err)) => return Err((variant(startable), format!("{}", err))),
    };

    match res {
        Ok(client) => Ok(GreetdState::NotStarted(client)),
        Err((startable, err)) => Err((variant(startable), format!("{}", err))),
    }
}

async fn try_autostart<Client>(
    state: GreetdState<Client>,
    command: Option<Vec<String>>,
    env: Vec<String>,
    sender: &AsyncComponentSender<AuthView<Client>>,
) -> GreetdState<Client>
where
    Client: Greetd,
{
    if let GreetdState::Startable(startable) = state {
        let Some(command) = command else {
            sender
                .output(AuthViewOutput::NotifyError(
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
