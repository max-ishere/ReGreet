use std::convert::Infallible;

use crate::greetd::CreateSessionResponse;
use crate::greetd_response;

use super::{AuthMessage, AuthResponse, CancellableSession, Greetd, StartableSession};

#[derive(Debug)]
pub struct DemoGreetd {}

impl Greetd for DemoGreetd {
    type StartableSession = Self;

    type AuthQuestion = Self;

    type AuthInformative = Self;

    type Error = Infallible;

    fn create_session(
        self,
        _username: &str,
    ) -> greetd_response!(Self, CreateSessionResponse<Self>) {
        async { Ok(Ok(CreateSessionResponse::AuthQuestion(self))) }
    }
}

impl CancellableSession for DemoGreetd {
    type Client = Self;

    fn cancel_session(
        self,
    ) -> greetd_response!(Self::Client, <Self as CancellableSession>::Client) {
        async { Ok(Ok(self)) }
    }
}

impl AuthResponse for DemoGreetd {
    type Client = Self;

    fn message(&self) -> AuthMessage<'_> {
        AuthMessage::Secret("Password")
    }

    fn respond(
        self,
        _msg: Option<String>,
    ) -> greetd_response!(
        <Self as AuthResponse>::Client,
        CreateSessionResponse<<Self as AuthResponse>::Client>
    ) {
        async { Ok(Ok(CreateSessionResponse::Success(self))) }
    }
}

impl StartableSession for DemoGreetd {
    type Client = Self;

    fn start_session(
        self,
        _cmd: Vec<String>,
        _env: Vec<String>,
    ) -> greetd_response!(
        <Self as StartableSession>::Client,
        <Self as StartableSession>::Client
    ) {
        async { Ok(Ok(self)) }
    }
}
