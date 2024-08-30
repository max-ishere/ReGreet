use std::convert::Infallible;

use crate::greetd::CreateSessionResponse;

use super::{
    AuthInformativeResponse, AuthMessage, AuthQuestionResponse, AuthResponse, CancellableSession,
    Greetd, StartableSession,
};

pub struct MockGreetd {}

impl Greetd for MockGreetd {
    type StartableSession = Self;

    type AuthQuestion = Self;

    type AuthInformative = Self;

    type Error = Infallible;

    async fn create_session(
        self,
        _username: &str,
    ) -> super::Response<Self, Self, super::CreateSessionResponse<Self>> {
        Ok(Ok(CreateSessionResponse::AuthQuestion(self)))
    }
}

impl CancellableSession for MockGreetd {
    type Client = Self;

    async fn cancel_session(
        self,
    ) -> super::Response<Self::Client, Self, <Self as CancellableSession>::Client> {
        Ok(Ok(self))
    }
}

impl AuthResponse for MockGreetd {
    type Client = Self;

    fn message<'a>(&'a self) -> AuthMessage<'a> {
        AuthMessage::Visible(
            "OTP Code (someone said theirs is long, so here we go, lorem ipsum dolor sit amet)",
        )
    }

    async fn respond(
        self,
        _msg: Option<String>,
    ) -> super::Response<
        <Self as AuthResponse>::Client,
        Self,
        CreateSessionResponse<<Self as AuthResponse>::Client>,
    > {
        Ok(Ok(CreateSessionResponse::Success(self)))
    }
}

impl AuthQuestionResponse for MockGreetd {
    type Client = Self;
}

impl AuthInformativeResponse for MockGreetd {
    type Client = Self;
}

impl StartableSession for MockGreetd {
    type Client = Self;

    async fn start_session(
        self,
        _cmd: Vec<String>,
        _env: Vec<String>,
    ) -> super::Response<<Self as StartableSession>::Client, Self, <Self as StartableSession>::Client>
    {
        Ok(Ok(self))
    }
}
