//! # Greetd IPC interactions
//!
//! This module contains a set of traits and their return types that can be used to talk to greetd. It also contains
//! implementations for types that are [`AsyncRead`]` + `[`AsyncWrite`] which allows the use of [`UnixStream`] etc. And
//! a demo implementation for use in development and testing.
//!
//! [`AsyncRead`]: tokio::io::AsyncRead
//! [`AsyncWrite`]: tokio::io::AsyncWrite
//! [`UnixStream`]: tokio::net::UnixStream
//!
//! ## Making Requests
//!
//! Requests are made by calling functions on traits. the returned values are the response types. Start with something
//! that implements [`Greetd`]. From there you can create a session using [`create_session`] and depending on the type
//! of response either start it, perform authentication or cancel a session.
//!
//! The trait set may look complicated, but it is used to keep track of the connection state. The assumption is - if
//! greetd returns an expected "ok" value, the interaction succeeded. If the response indicates an error, the state is
//! assumed to stay in the same state as it was.
//!
//! Brief description of each trait:
//!
//! - [`Greetd`] - The starting point
//! - [`StartableSession`] - A session that can be started without further authentication
//! - [`AuthResponse`] - A generic auth prompt. Used to auto implement [`AuthQuestionResponse`]
//!   and [`AuthInformativeResponse`].
//!   - [`AuthQuestionResponse`] - A specialized auth prompt that has to be answered with a message.
//!   - [`AuthInformativeResponse`] - A specialized auth prompt that doesn't require a message.
//! - [`CancellableSession`] - Avaliable as part of all traits except [`Greetd`].
//!
//! [`create_session`]: Greetd::create_session
//!
//! ## Resolving traits to concrete types
//!
//! This module allows you to use generics with trait bounds that will always resolve to a concrete type. This way, you
//! see a generic with methods to change it into a different generic, but the compiler sees that as soon as the `Greetd`
//! type is specified, all other types can be resolved using the associated types.
//!
//! ## Handling responses
//!
//! Each IPC method uses the [`Response`] type alias. This alias represents a nested enum like this:
//!
//! - [`Err`]: Current state + IO Error (comes from [`Greetd::Error`])
//! - [`Ok`]: No IPC IO errors, but:
//!   - [`Err`]: Current state + error reported by greetd
//!   - [`Ok`]: Everything ok, contains the new state.

#![allow(clippy::manual_async_fn)]

use std::fmt::{Debug, Display};

use thiserror::Error;

#[doc(hidden)]
pub use demo::*;

#[doc(hidden)]
mod async_sock_impl;
#[doc(hidden)]
mod demo;

/// A nested [`Result`] to represent errors occuring in IPC interactions
///
/// The types go like this:
/// 1. The client so it's error can be used as the outter result's Err
/// 2. The current type if the interaction fails (always `Self` basically). Present as `.0` in both variants `Err` so
///    that the interaction can be retried.
/// 3. The next type if the interaction succeeds (the `Ok(Ok(Next))` case).
type Response<Client, Current, Next> =
    Result<Result<Next, (Current, RequestError)>, (Current, <Client as Greetd>::Error)>;

/// A macro to generate the return type for IPC interactions.
///
/// # Usage
///
/// ```rust
/// # use regreet::greetd_response;
/// # type TypeOnSuccess = ();
/// #
/// fn foo() -> greetd_response!(Client, TypeOnSuccess) {
///     async { Ok(Ok(TypeOnSuccess)) }
/// }
/// ```
#[macro_export]
macro_rules! greetd_response {
    ($client:ty, $next:ty) => {
        impl std::future::Future<Output = $crate::greetd::Response<$client, Self, $next>> + std::marker::Send
    };
}

/// A client that can create a session.
pub trait Greetd: Sized + Send
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

    /// Communication errors with Greetd over IPC. The [`Display`] requirement comes from the need to write the error
    /// text in the UI.
    type Error: Debug + Display;

    fn create_session(self, username: &str) -> greetd_response!(Self, CreateSessionResponse<Self>);
}

/// A generic authentication request. Store [`AuthQuestionResponse`] or [`AuthInformativeResponse`] instead.
pub trait AuthResponse: CancellableSession + Sized {
    type Client: Greetd;

    /// Returns the message sent by greetd. The message is cached and doesn't cause any IPC IO.
    fn message(&self) -> AuthMessage<'_>;

    /// Send a response to this message over IPC.
    fn respond(
        self,
        msg: Option<String>,
    ) -> greetd_response!(
        <Self as AuthResponse>::Client,
        CreateSessionResponse<<Self as AuthResponse>::Client>
    );
}

/// A question with an answer being some kind of credential.
pub trait AuthQuestionResponse: AuthResponse {
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

impl<T> AuthQuestionResponse for T
where
    T: AuthResponse,
{
    type Client = <T as AuthResponse>::Client;
}

/// A message that simply requires acknoledgement from the user, but no input.
pub trait AuthInformativeResponse: AuthResponse {
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

impl<T> AuthInformativeResponse for T
where
    T: AuthResponse,
{
    type Client = <T as AuthResponse>::Client;
}

/// An IPC state where a session can be started.
pub trait StartableSession: CancellableSession + Send {
    type Client: Greetd;

    fn start_session(
        self,
        cmd: Vec<String>,
        env: Vec<String>,
    ) -> greetd_response!(
        <Self as StartableSession>::Client,
        <Self as StartableSession>::Client
    );
}

/// An IPC state where a session can be canceled.
pub trait CancellableSession: Sized + Send {
    type Client: Greetd;

    fn cancel_session(self)
        -> greetd_response!(Self::Client, <Self as CancellableSession>::Client);
}

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
