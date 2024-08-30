use std::mem::{self, replace, take};

use derivative::Derivative;
use gtk4::prelude::*;
use relm4::component::{AsyncComponentParts, SimpleAsyncComponent};
use relm4::{prelude::*, AsyncComponentSender};
use replace_with::replace_with_or_abort;
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use tracing::{debug, error};

use crate::greetd::{
    AuthInformative, AuthInformativeResponse, AuthQuestion, AuthQuestionResponse, AuthResponse,
    CancellableSession, CreateSessionResponse, Greetd, StartableSession,
};
use crate::gui::templates::LoginButton;

pub struct AuthViewInit<Client>
where
    Client: Greetd + 'static,
{
    pub greetd_state: GreetdState<Client>,
    pub username: String,
    pub command: Vec<String>,
    pub env: Vec<String>,
}

pub struct AuthView<Client>
where
    Client: Greetd + 'static,
{
    greetd_state: GreetdState<Client>,
    username: String,
    command: Vec<String>,
    env: Vec<String>,
}
pub enum GreetdState<Client>
where
    Client: Greetd,
{
    NotStarted(Client),
    Startable(Client::StartableSession),
    AuthQuestion {
        session: Client::AuthQuestion,
        credential: String,
    },
    AuthInformative(Client::AuthInformative),
    Loading(
        /// Message shown while loading
        String,
    ),
}

#[derive(Debug)]
pub enum AuthViewOutput {
    /// Tell the parent to show an error that occured during authentication
    NotifyError(String),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum AuthViewMsg {
    CredentialChanged(#[derivative(Debug = "ignore")] String),

    Cancel,
    Submit,
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
            #[transition = "OverDown"]
            match &model.greetd_state {
                GreetdState::NotStarted(_) => gtk::Box {
                    set_halign: gtk::Align::End,
                    #[template] LoginButton { connect_clicked => AuthViewMsg::Submit },
                }
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
                        #[template] LoginButton { connect_clicked => AuthViewMsg::Submit },
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

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
                        }
                        AuthQuestion::Visible(prompt) => gtk::Entry {
                            #[watch]
                            set_placeholder_text: Some(prompt),

                            connect_changed[sender] => move |this| sender.input(Self::Input::CredentialChanged(this.text().to_string())),
                        }
                    },

                    #[template]
                    append = &LoginBox {
                        #[template] CancelButton { connect_clicked => AuthViewMsg::Cancel },
                        #[template] LoginButton { connect_clicked => AuthViewMsg::Submit },
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
                        #[template] LoginButton { connect_clicked => AuthViewMsg::Submit },
                    }
                }

                GreetdState::Loading(message) => gtk::Box {
                    set_spacing: 15,

                    gtk::Label {
                        #[watch]
                        set_text: message.as_str(),
                        set_valign: gtk::Align::Start,
                    },

                    append = &gtk::ProgressBar {
                        pulse: (),
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
            command,
            env,
        };
        let widgets = view_output!();

        // Note: For some reason in post_view() this didnt work.
        widgets.auth_conditional.set_vhomogeneous(false);
        widgets.auth_conditional.set_interpolate_size(true);

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>) {
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
            AuthViewMsg::Submit => self.progress_login(&sender).await,
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

    pub async fn progress_login(&mut self, sender: &AsyncComponentSender<Self>) {
        use GreetdState as S;

        let greetd_state = replace(
            &mut self.greetd_state,
            S::Loading(String::from("Canceling session")),
        );

        let maybe_startable = match greetd_state {
            S::Loading(old) => S::Loading(old),

            GreetdState::NotStarted(client) => {
                report_error(try_create_session(client, &self.username).await, sender)
            }
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
                )
                .await,
                sender,
            ),
            GreetdState::AuthInformative(informative) => report_error(
                try_auth(informative, S::AuthInformative, None).await,
                sender,
            ),
        };

        self.greetd_state = try_autostart(maybe_startable, &self.command, &self.env, sender).await;
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
        R::AuthQuestion(question) => GreetdState::AuthQuestion {
            session: question,
            credential: String::new(),
        },
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
    command: &[String],
    env: &[String],
    sender: &AsyncComponentSender<AuthView<Client>>,
) -> GreetdState<Client>
where
    Client: Greetd,
{
    if let GreetdState::Startable(startable) = state {
        report_error(
            try_start_session(
                startable,
                GreetdState::<Client>::Startable,
                command.to_owned(),
                env.to_owned(),
            )
            .await,
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
) -> Result<
    GreetdState<<Message as AuthResponse>::Client>,
    (GreetdState<<Message as AuthResponse>::Client>, String),
>
where
    Message: AuthResponse,
{
    let session = block_in_place(move || {
        Handle::current().block_on(async { message.respond(credential).await })
    });

    let res = match session {
        Ok(res) => res,
        Err((message, err)) => return Err((variant(message), format!("{}", err))),
    };

    let session = match res {
        Ok(session) => session,
        Err((message, err)) => return Err((variant(message), format!("{}", err))),
    };

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
