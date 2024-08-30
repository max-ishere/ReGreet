use derivative::Derivative;
use gtk4::prelude::*;
use relm4::component::{AsyncComponent as _, AsyncComponentController as _, AsyncController};
use relm4::prelude::*;

use crate::greetd::Greetd;
use crate::gui::component::auth_view::AuthViewInit;
use crate::gui::component::{AuthViewOutput, SelectorInit, SelectorOutput};
use crate::gui::templates::EntryLabel;

use super::auth_view::{AuthView, GreetdState};
use super::{EntryOrDropDown, Selector, SelectorOption};

const LABEL_HEIGHT_REQUEST: i32 = 45;

const USER_ROW: i32 = 0;
const SESSION_ROW: i32 = 1;
const AUTH_ROW: i32 = 2;

pub struct AuthUiInit<Client>
where
    Client: Greetd,
{
    pub users: Vec<SelectorOption>,
    pub initial_user: EntryOrDropDown,

    pub sessions: Vec<SelectorOption>,
    pub initial_session: EntryOrDropDown,

    pub greetd_state: GreetdState<Client>,
}

pub struct AuthUi<Client>
where
    Client: Greetd + 'static,
{
    selected_user: EntryOrDropDown,
    selected_session: EntryOrDropDown,

    #[doc(hidden)]
    user_selector: Controller<Selector>,
    #[doc(hidden)]
    session_selector: Controller<Selector>,
    #[doc(hidden)]
    auth_view: AsyncController<AuthView<Client>>,
}

#[derive(Debug)]
pub enum AuthUiOutput {}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum AuthUiMsg {
    UserChanged(EntryOrDropDown),
    SessionChanged(EntryOrDropDown),
    ShowError(String),
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
            attach[0, USER_ROW, 1, 1] =  &EntryLabel {
                set_label: "User:",
                set_height_request: LABEL_HEIGHT_REQUEST,
            },

            attach[1, USER_ROW, 1, 1] = model.user_selector.widget(),

            #[template]
            attach[0, SESSION_ROW, 1, 1] = &EntryLabel {
                set_label: "Session:",
                set_height_request: LABEL_HEIGHT_REQUEST,
            },

            // TODO: Change the session when the user is changed.
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
            users,
            initial_user,
            sessions,
            initial_session,
            greetd_state,
        } = init;

        let username = match &initial_user {
            EntryOrDropDown::Entry(username) => username,
            EntryOrDropDown::DropDown(username) => username,
        }
        .to_string();

        let user_selector = Selector::builder()
            .launch(SelectorInit {
                entry_placeholder: "System username".to_string(),
                options: users.clone(),
                initial_selection: initial_user.clone(),
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
                options: sessions.clone(),
                initial_selection: initial_session.clone(),
                toggle_icon_name: "document-edit-symbolic".to_string(),
                toggle_tooltip: "Manually enter session command".to_string(),
            })
            .forward(sender.input_sender(), move |output| {
                let SelectorOutput::CurrentSelection(selection) = output;

                Self::Input::SessionChanged(selection)
            });

        let auth_view = AuthView::builder()
            .launch(AuthViewInit {
                greetd_state,
                username: username,
                // TODO: Use real command and vec.
                command: Vec::new(),
                env: Vec::new(),
            })
            .forward(sender.input_sender(), move |output| {
                let AuthViewOutput::NotifyError(error) = output;

                Self::Input::ShowError(error)
            });

        let model = Self {
            user_selector,
            selected_user: initial_user,

            session_selector,
            selected_session: initial_session,

            auth_view,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}
