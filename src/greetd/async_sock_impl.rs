//! An implementation of greetd client for types that can do async IO.
//!
//! ## Logging policy
//!
//! This module uses the debug level for logging in order to not store a long authentication history in real use.
use std::marker::Unpin;

use greetd_ipc::{codec::TokioCodec, AuthMessageType, ErrorType, Request, Response};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::debug;

use crate::{greetd::RequestError, greetd_response};

use super::{AuthResponse, CancellableSession, CreateSessionResponse, Greetd, StartableSession};

/// A marker trait for types that can do async IO.
pub(crate) trait TokioRW: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T> TokioRW for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub struct AuthMessage<RW>
where
    RW: TokioRW,
{
    rw: RW,
    message: String,
    r#type: AuthMessageType,
}

impl<RW> AuthMessage<RW>
where
    RW: TokioRW,
{
    pub fn new_as_create_session(
        rw: RW,
        r#type: AuthMessageType,
        message: String,
    ) -> CreateSessionResponse<RW> {
        match r#type {
            AuthMessageType::Visible | AuthMessageType::Secret => {
                CreateSessionResponse::AuthQuestion(Self {
                    rw,
                    message,
                    r#type,
                })
            }
            AuthMessageType::Info | AuthMessageType::Error => {
                CreateSessionResponse::AuthInformative(Self {
                    rw,
                    message,
                    r#type,
                })
            }
        }
    }
}

impl<RW> Greetd for RW
where
    RW: TokioRW,
    RW: CancellableSession<Client = RW> + StartableSession<Client = RW>,
{
    type StartableSession = RW;
    type AuthQuestion = AuthMessage<RW>;
    type AuthInformative = AuthMessage<RW>;
    type Error = greetd_ipc::codec::Error;

    fn create_session(self, username: &str) -> greetd_response!(Self, CreateSessionResponse<Self>) {
        async move {
            debug!("Creating session for user: {username}");

            let (self_, response) = make_request(
                self,
                Request::CreateSession {
                    username: username.to_string(),
                },
            )
            .await?;

            Ok(match response {
                Response::Success => Ok(CreateSessionResponse::Success(self_)),
                Response::Error {
                    error_type,
                    description,
                } => Err((
                    self_,
                    match error_type {
                        ErrorType::Error => RequestError::Error(description),
                        ErrorType::AuthError => RequestError::Auth(description),
                    },
                )),
                Response::AuthMessage {
                    auth_message_type,
                    auth_message,
                } => Ok(AuthMessage::new_as_create_session(
                    self_,
                    auth_message_type,
                    auth_message,
                )),
            })
        }
    }
}

impl<RW> StartableSession for RW
where
    RW: TokioRW,
    RW: CancellableSession<Client = RW>,
{
    type Client = RW;

    fn start_session(
        self,
        cmd: Vec<String>,
        env: Vec<String>,
    ) -> greetd_response!(
        <Self as StartableSession>::Client,
        <Self as StartableSession>::Client
    ) {
        async {
            debug!("Starting session with command {cmd:?}",);

            let ok = |t| Ok(Ok(t));

            let (client, response) = make_request(self, Request::StartSession { cmd, env }).await?;

            match response {
                Response::Success => ok(client),
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description,
                } => Ok(Err((client, super::RequestError::Auth(description)))),
                Response::Error {
                    error_type: ErrorType::Error,
                    description,
                } => Ok(Err((client, super::RequestError::Error(description)))),
                Response::AuthMessage { .. } => unreachable!(
                    "greetd responded with auth request when starting an authenticated session"
                ),
            }
        }
    }
}

impl<T> CancellableSession for T
where
    T: TokioRW,
{
    type Client = Self;

    async fn cancel_session(
        self,
    ) -> super::Response<Self::Client, Self, <Self as CancellableSession>::Client> {
        debug!("Cancelling session");

        let (client, response) = make_request(self, Request::CancelSession).await?;

        match response {
            Response::Success => Ok(Ok(client)),
            Response::Error {
                error_type: ErrorType::AuthError,
                description,
            } => Ok(Err((client, super::RequestError::Auth(description)))),
            Response::Error {
                error_type: ErrorType::Error,
                description,
            } => Ok(Err((client, super::RequestError::Error(description)))),
            Response::AuthMessage { .. } => {
                unreachable!("greetd responded with an auth prompt for canceling a session")
            }
        }
    }
}

impl<RW> AuthResponse for AuthMessage<RW>
where
    RW: TokioRW,
{
    type Client = RW;

    fn message(&self) -> super::AuthMessage<'_> {
        use super::AuthMessage as R;
        match self.r#type {
            AuthMessageType::Visible => R::Visible(&self.message),
            AuthMessageType::Secret => R::Secret(&self.message),
            AuthMessageType::Info => R::Info(&self.message),
            AuthMessageType::Error => R::Error(&self.message),
        }
    }

    fn respond(
        mut self,
        msg: Option<String>,
    ) -> greetd_response!(
        <Self as AuthResponse>::Client,
        CreateSessionResponse<<Self as AuthResponse>::Client>
    ) {
        async {
            let request = Request::PostAuthMessageResponse { response: msg };
            if let Err(e) = request.write_to(&mut self.rw).await {
                return Err((self, e));
            }

            let response = match Response::read_from(&mut self.rw).await {
                Ok(r) => r,
                Err(e) => return Err((self, e)),
            };

            Ok(match response {
                Response::Success => Ok(CreateSessionResponse::Success(self.rw)),
                Response::Error {
                    error_type: ErrorType::AuthError,
                    description,
                } => Err((self, super::RequestError::Auth(description))),
                Response::Error {
                    error_type: ErrorType::Error,
                    description,
                } => Err((self, super::RequestError::Error(description))),
                Response::AuthMessage {
                    auth_message_type,
                    auth_message,
                } => Ok(Self::new_as_create_session(
                    self.rw,
                    auth_message_type,
                    auth_message,
                )),
            })
        }
    }
}

impl<RW> CancellableSession for AuthMessage<RW>
where
    RW: TokioRW,
{
    type Client = RW;

    fn cancel_session(
        self,
    ) -> greetd_response!(Self::Client, <Self as CancellableSession>::Client) {
        async {
            let result = match self.rw.cancel_session().await {
                Ok(res) => res,
                Err((not_canceled, e)) => {
                    return Err((
                        Self {
                            rw: not_canceled,
                            ..self
                        },
                        e,
                    ))
                }
            };

            Ok(match result {
                Ok(rw) => Ok(rw),
                Err((not_canceled, e)) => Err((
                    Self {
                        rw: not_canceled,
                        ..self
                    },
                    e,
                )),
            })
        }
    }
}

/// Consumes self and the request and maps the greetd errors in such a way that on failure, self is returned in the error side
async fn make_request<RW>(
    mut rw: RW,
    request: Request,
) -> Result<(RW, Response), (RW, greetd_ipc::codec::Error)>
where
    RW: TokioRW,
{
    if let Err(e) = request.write_to(&mut rw).await {
        return Err((rw, e));
    }

    match Response::read_from(&mut rw).await {
        Ok(r) => Ok((rw, r)),
        Err(e) => Err((rw, e)),
    }
}