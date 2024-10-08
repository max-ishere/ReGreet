use std::collections::HashMap;
use std::fmt::Debug;
use std::mem::take;

use anyhow::Context;
use derivative::Derivative;
use relm4::{gtk::prelude::*, prelude::*};
use tracing::error;

use crate::cache::SessionIdOrCmdline;
use crate::constants::{CACHE_LIMIT, CACHE_PATH};
use crate::sysutil::SessionInfo;
use crate::{cache::Cache, greetd::Greetd};

use super::{
    EntryOrDropDown, GreetdControls, GreetdControlsInit, GreetdControlsMsg, GreetdControlsOutput,
    GreetdState, Selector, SelectorInit, SelectorMsg, SelectorOption, SelectorOutput,
};

const USER_ROW: i32 = 0;
const SESSION_ROW: i32 = 1;
const AUTH_ROW: i32 = 2;

pub struct AuthUiInit<Client>
where
    Client: Greetd,
{
    pub users: HashMap<String, String>,
    pub sessions: HashMap<String, SessionInfo>,
    pub env: HashMap<String, String>,

    pub initial_user: String,
    pub cache: Cache,

    pub greetd_state: GreetdState<Client>,
}

pub struct AuthUi<Client>
where
    Client: Greetd + 'static + Debug,
{
    cache: Cache,

    current_username: String,
    current_session: EntryOrDropDown,

    #[doc(hidden)]
    user_selector: Controller<Selector>,
    #[doc(hidden)]
    session_selector: Controller<Selector>,
    #[doc(hidden)]
    auth_view: Controller<GreetdControls<Client>>,
}

#[derive(Debug)]
pub enum AuthUiOutput {
    ShowError(String),
    SessionStarted,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum AuthUiMsg {
    UserChanged(EntryOrDropDown),
    SessionChanged(Option<Vec<String>>),
    ShowError(String),

    LockUserSelectors,
    UnlockUserSelectors,

    SessionStarted,
}

#[relm4::component(pub)]
impl<Client> SimpleComponent for AuthUi<Client>
where
    Client: Greetd + 'static + Debug,
{
    type Init = AuthUiInit<Client>;
    type Input = AuthUiMsg;
    type Output = AuthUiOutput;

    view! {
        gtk::Grid {
            set_column_spacing: 15,
            set_row_spacing: 15,

            #[template]
            attach[0, USER_ROW, 1, 1] =  &SelectorLabel {
                set_label: "User",
            },

            attach[1, USER_ROW, 1, 1] = model.user_selector.widget(),

            #[template]
            attach[0, SESSION_ROW, 1, 1] = &SelectorLabel {
                set_label: "Session",
            },

            attach[1, SESSION_ROW, 1, 1] = model.session_selector.widget(),

            attach[0, AUTH_ROW, 2, 1] = model.auth_view.widget(),
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let AuthUiInit {
            sessions,
            users,
            env,

            initial_user,
            cache,

            greetd_state,
        } = init;

        let initial_session = cache
            .last_user()
            .and_then(|user| cache.last_user_session(user))
            .and_then(|session| match session {
                SessionIdOrCmdline::XdgDektopFile(id) => sessions
                    .contains_key(id)
                    .then_some(EntryOrDropDown::DropDown(id.clone())),
                SessionIdOrCmdline::Command(cmd) => Some(EntryOrDropDown::Entry(cmd.clone())),
            })
            .unwrap_or(
                sessions
                    .keys()
                    .next()
                    .map(|id| EntryOrDropDown::DropDown(id.clone()))
                    .unwrap_or_else(|| EntryOrDropDown::Entry(String::new())),
            );

        let user_entry = if users.contains_key(&initial_user) {
            EntryOrDropDown::DropDown(initial_user.clone())
        } else {
            EntryOrDropDown::Entry(initial_user.clone())
        };

        let user_options = users
            .into_iter()
            .map(|(system, display)| SelectorOption {
                id: system,
                text: display,
            })
            .collect();

        let initial_command = sessions
            .get(&initial_user)
            .map(|sess| sess.command.clone())
            .unwrap_or_default();

        let user_selector = Selector::builder()
            .launch(SelectorInit {
                entry_placeholder: "System username".to_string(),
                options: user_options,
                initial_selection: user_entry,
                locked: !matches!(greetd_state, GreetdState::NotCreated(_)),
                toggle_icon_name: "document-edit-symbolic".to_string(),
                toggle_tooltip: "Manually enter a system username".to_string(),
            })
            .forward(sender.input_sender(), move |output| {
                let SelectorOutput::CurrentSelection(selection) = output;

                Self::Input::UserChanged(selection)
            });

        let session_selector = Selector::builder()
            .launch(SelectorInit {
                entry_placeholder: "Session command".to_string(),
                options: sessions
                    .iter()
                    .map(|(xdg_id, SessionInfo { name, .. })| SelectorOption {
                        id: xdg_id.clone(),
                        text: name.clone(),
                    })
                    .collect(),
                initial_selection: initial_session.clone(),
                locked: false,
                toggle_icon_name: "document-edit-symbolic".to_string(),
                toggle_tooltip: "Manually enter session command".to_string(),
            })
            .forward(sender.input_sender(), move |output| {
                let SelectorOutput::CurrentSelection(entry) = output;
                let cmdline = match entry {
                    EntryOrDropDown::Entry(cmdline) => shlex::split(&cmdline),
                    EntryOrDropDown::DropDown(id) => sessions
                        .get(&id)
                        .map(|SessionInfo { command, .. }| command.clone()),
                };

                Self::Input::SessionChanged(cmdline)
            });

        let auth_view = GreetdControls::builder()
            .launch(GreetdControlsInit {
                greetd_state,
                username: initial_user.clone(),
                command: initial_command,
                env: env.into_iter().map(|(k, v)| format!("{k}={v}")).collect(),
            })
            .forward(sender.input_sender(), move |output| {
                use AuthUiMsg as I;
                use GreetdControlsOutput as O;

                match output {
                    O::NotifyError(error) => I::ShowError(error),
                    O::LockUserSelectors => I::LockUserSelectors,
                    O::UnlockUserSelectors => I::UnlockUserSelectors,
                    O::SessionStarted => I::SessionStarted,
                }
            });

        let model = Self {
            cache,

            current_username: initial_user,
            current_session: initial_session,

            user_selector,
            session_selector,
            auth_view,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        use AuthUiMsg as I;
        match message {
            I::UserChanged(entry) => {
                let username = match entry {
                    EntryOrDropDown::DropDown(username) => username,
                    EntryOrDropDown::Entry(username) => username,
                };
                self.auth_view
                    .emit(GreetdControlsMsg::UpdateUser(username.clone()));

                let Some(last_session) = self.cache.last_user_session(&username) else {
                    return;
                };

                self.session_selector
                    .emit(SelectorMsg::Set(match last_session {
                        SessionIdOrCmdline::Command(cmd) => EntryOrDropDown::Entry(cmd.clone()),
                        SessionIdOrCmdline::XdgDektopFile(id) => {
                            EntryOrDropDown::DropDown(id.clone())
                        }
                    }));
            }

            I::SessionChanged(entry) => {
                self.auth_view.emit(GreetdControlsMsg::UpdateSession(entry))
            }

            I::LockUserSelectors => self.user_selector.emit(SelectorMsg::Lock),
            I::UnlockUserSelectors => self.user_selector.emit(SelectorMsg::Unlock),

            I::ShowError(error) => {
                error!("ShowError messsage: {error}");

                sender.output(AuthUiOutput::ShowError(error)).unwrap();
            }

            I::SessionStarted => {
                self.user_selector.emit(SelectorMsg::Lock);
                self.session_selector.emit(SelectorMsg::Lock);

                self.cache.set_last_login(
                    self.current_username.clone(),
                    match self.current_session.clone() {
                        EntryOrDropDown::Entry(cmd) => SessionIdOrCmdline::Command(cmd),
                        EntryOrDropDown::DropDown(id) => SessionIdOrCmdline::XdgDektopFile(id),
                    },
                );

                let cache = take(&mut self.cache);
                let send = sender.clone();
                sender.command(move |_, _| async move {
                    let Err(e) = cache
                        .save(CACHE_PATH, CACHE_LIMIT)
                        .await
                        .with_context(|| format!("Failed to save the cache file `{CACHE_PATH}`"))
                    else {
                        return;
                    };

                    error!("{e:?}");

                    send.output(AuthUiOutput::SessionStarted).unwrap();
                })
            }
        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for SelectorLabel {
    view! {
        gtk::Label {
            set_xalign: 1.0,
        }
    }
}
