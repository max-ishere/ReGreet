pub use mock::*;
use std::fmt::{Debug, Display};
use thiserror::Error;

#[doc(hidden)]
mod async_sock_impl;
#[doc(hidden)]
mod mock;

type Response<Client, Current, Next> =
    Result<Result<Next, (Current, RequestError)>, (Current, <Client as Greetd>::Error)>;

// Requests

pub trait Greetd: Sized
where
    Self::StartableSession: CancellableSession<Client = Self>,
    Self::StartableSession: StartableSession<Client = Self>,

    Self::AuthQuestion: CancellableSession<Client = Self>,
    Self::AuthQuestion: AuthInformativeResponse<Client = Self>,
    Self::AuthQuestion: AuthResponse<Client = Self>,

    Self::AuthInformative: CancellableSession<Client = Self>,
    Self::AuthInformative: AuthInformativeResponse<Client = Self>,
    Self::AuthInformative: AuthResponse<Client = Self>,
{
    type StartableSession: StartableSession;
    type AuthQuestion: AuthQuestionResponse;
    type AuthInformative: AuthInformativeResponse;

    type Error: Debug + Display;

    async fn create_session(
        self,
        username: &str,
    ) -> Response<Self, Self, CreateSessionResponse<Self>>;
}

pub trait AuthResponse: CancellableSession + Sized {
    type Client: Greetd;

    fn message(&self) -> AuthMessage<'_>;
    async fn respond(
        self,
        msg: Option<String>,
    ) -> Response<
        <Self as AuthResponse>::Client,
        Self,
        CreateSessionResponse<<Self as AuthResponse>::Client>,
    >;
}

pub trait AuthQuestionResponse: CancellableSession + AuthResponse {
    type Client: Greetd;

    fn auth_question(&self) -> AuthQuestion {
        match self.message() {
            AuthMessage::Visible(message) => AuthQuestion::Visible(message),
            AuthMessage::Secret(message) => AuthQuestion::Secret(message),
            AuthMessage::Info(_) | AuthMessage::Error(_) => {
                unreachable!("auth question cannot be an error or info")
            }
        }
    }
}

pub trait AuthInformativeResponse: CancellableSession + AuthResponse {
    type Client: Greetd;

    fn auth_informative(&self) -> AuthInformative<'_> {
        match self.message() {
            AuthMessage::Info(message) => AuthInformative::Info(message),
            AuthMessage::Error(message) => AuthInformative::Error(message),
            AuthMessage::Visible(_) | AuthMessage::Secret(_) => {
                unreachable!("auth informative cannot be a visible or a secret question")
            }
        }
    }
}

pub trait StartableSession: CancellableSession {
    type Client: Greetd;

    async fn start_session(
        self,
        cmd: Vec<String>,
        env: Vec<String>,
    ) -> Response<<Self as StartableSession>::Client, Self, <Self as StartableSession>::Client>;
}

pub trait CancellableSession: Sized {
    type Client: Greetd;

    async fn cancel_session(
        self,
    ) -> Response<Self::Client, Self, <Self as CancellableSession>::Client>;
}

// Responses

pub enum CreateSessionResponse<Client>
where
    Client: Greetd,
    Client::StartableSession: StartableSession,
    Client::AuthQuestion: AuthQuestionResponse,
    Client::AuthInformative: AuthInformativeResponse,
{
    Success(Client::StartableSession),
    AuthQuestion(Client::AuthQuestion),
    AuthInformative(Client::AuthInformative),
}

pub enum AuthMessage<'a> {
    Visible(&'a str),
    Secret(&'a str),
    Info(&'a str),
    Error(&'a str),
}

pub enum AuthQuestion<'a> {
    Visible(&'a str),
    Secret(&'a str),
}

impl<'a> AuthQuestion<'a> {
    pub fn prompt(&self) -> &str {
        match self {
            AuthQuestion::Visible(prompt) => prompt,
            AuthQuestion::Secret(prompt) => prompt,
        }
    }
}

pub enum AuthInformative<'a> {
    Info(&'a str),
    Error(&'a str),
}

impl<'a> AuthInformative<'a> {
    pub fn prompt(&self) -> &str {
        match self {
            AuthInformative::Info(prompt) => prompt,
            AuthInformative::Error(prompt) => prompt,
        }
    }
}

#[derive(Error, Debug)]
pub enum RequestError {
    #[error("Greetd error: {0}")]
    Error(String),
    #[error("Greetd authentication error: {0}")]
    Auth(String),
}
