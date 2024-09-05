use std::collections::HashMap;

use derivative::Derivative;
use gtk4::prelude::*;
use relm4::component::{AsyncComponent as _, AsyncComponentController as _, AsyncController};
use relm4::prelude::*;
use tracing::error;

use crate::greetd::Greetd;
use crate::gui::component::greetd_controls::GreetdControlsInit;
use crate::gui::component::{GreetdControlsOutput, SelectorInit, SelectorMsg, SelectorOutput};
use crate::sysutil::SessionInfo;

use super::greetd_controls::{GreetdControls, GreetdState};
use super::{EntryOrDropDown, GreetdControlsMsg, Selector, SelectorOption};

const USER_ROW: i32 = 0;
const SESSION_ROW: i32 = 1;
const AUTH_ROW: i32 = 2;

pub struct AuthUiInit<Client>
where
    Client: Greetd,
{
    pub initial_user: String,
    pub users: HashMap<String, String>,
    pub sessions: HashMap<String, SessionInfo>,

    pub last_user_session_cache: HashMap<String, EntryOrDropDown>,

    pub greetd_state: GreetdState<Client>,
}

pub struct AuthUi<Client>
where
    Client: Greetd + 'static,
{
    last_user_session_cache: HashMap<String, EntryOrDropDown>,

    #[doc(hidden)]
    user_selector: Controller<Selector>,
    #[doc(hidden)]
    session_selector: Controller<Selector>,
    #[doc(hidden)]
    auth_view: AsyncController<GreetdControls<Client>>,
}

#[derive(Debug)]
pub enum AuthUiOutput {}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum AuthUiMsg {
    UserChanged(EntryOrDropDown),
    SessionChanged(Option<Vec<String>>),
    ShowError(String),

    LockUserSelectors,
    UnlockUserSelectors,
}

#[relm4::component(pub)]
impl<Client> SimpleComponent for AuthUi<Client>
where
    Client: Greetd + 'static,
{
    type Init = AuthUiInit<Client>;
    type Input = AuthUiMsg;
    type Output = AuthUiOutput;

    view! {
        gtk::Grid {
            set_column_spacing: 15,
            set_row_spacing: 15,
            set_margin_all: 15,

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
            initial_user,

            last_user_session_cache,

            greetd_state,
        } = init;

        let initial_session = last_user_session_cache
            .get(&initial_user)
            .and_then(|entry| {
                if let EntryOrDropDown::DropDown(id) = entry {
                    sessions.contains_key(id).then_some(entry)
                } else {
                    Some(entry)
                }
            })
            .cloned()
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
                initial_selection: initial_session,
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
                username: initial_user,
                // TODO: Use real command and vec.
                command: Vec::new(),
                env: Vec::new(),
            })
            .forward(sender.input_sender(), move |output| {
                use AuthUiMsg as I;
                use GreetdControlsOutput as O;

                match output {
                    O::NotifyError(error) => I::ShowError(error),
                    O::LockUserSelectors => I::LockUserSelectors,
                    O::UnlockUserSelectors => I::UnlockUserSelectors,
                }
            });

        let model = Self {
            last_user_session_cache,

            user_selector,
            session_selector,
            auth_view,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        use AuthUiMsg as I;
        match message {
            I::UserChanged(entry) => {
                let username = match entry {
                    EntryOrDropDown::DropDown(username) => username,
                    EntryOrDropDown::Entry(username) => username,
                };
                self.auth_view
                    .emit(GreetdControlsMsg::UpdateUser(username.clone()));

                let Some(last_session) = self.last_user_session_cache.get(&username) else {
                    return;
                };

                self.session_selector
                    .emit(SelectorMsg::Set(last_session.clone()));
            }

            I::SessionChanged(entry) => {
                self.auth_view.emit(GreetdControlsMsg::UpdateSession(entry))
            }

            I::LockUserSelectors => self.user_selector.emit(SelectorMsg::Lock),
            I::UnlockUserSelectors => self.user_selector.emit(SelectorMsg::Unlock),

            I::ShowError(error) => error!("{error}"), // TODO: Show an error
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
