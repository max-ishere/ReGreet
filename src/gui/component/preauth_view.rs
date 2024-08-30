use gtk4::prelude::*;
use relm4::prelude::*;

use crate::gui::component::{SelectorInit, SelectorOutput};
use crate::gui::templates::{EntryLabel, LoginButton};

use super::{EntryOrDropDown, Selector, SelectorOption};

const USER_ROW: i32 = 0;
const SESSION_ROW: i32 = 1;
const SUBMIT_ROW: i32 = 2;
const LABEL_HEIGHT_REQUEST: i32 = 45;

pub struct PreAuthViewInit {
    pub users: Vec<SelectorOption>,
    pub initial_user: EntryOrDropDown,

    pub sessions: Vec<SelectorOption>,
    pub initial_session: EntryOrDropDown,
}

pub struct PreAuthView {
    selected_user: EntryOrDropDown,
    selected_session: EntryOrDropDown,

    #[doc(hidden)]
    user_selector: Controller<Selector>,
    #[doc(hidden)]
    session_selector: Controller<Selector>,
}

#[derive(Debug)]
pub struct PreAuthViewOutput {
    pub user: EntryOrDropDown,
    pub session: EntryOrDropDown,
}

#[derive(Debug)]
pub enum PreAuthViewMsg {
    UserSelected(EntryOrDropDown),
    SessionSelected(EntryOrDropDown),
    Submit,
}

#[relm4::component(pub)]
impl SimpleComponent for PreAuthView {
    type Init = PreAuthViewInit;
    type Input = PreAuthViewMsg;
    type Output = PreAuthViewOutput;

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

            attach[1, SESSION_ROW, 1, 1] = model.session_selector.widget(),

            attach[0, SUBMIT_ROW, 2, 1 ] = &gtk::Box {
                set_halign: gtk::Align::End,
                #[template]
                LoginButton {
                    connect_clicked => Self::Input::Submit,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let PreAuthViewInit {
            users,
            initial_user,
            sessions,
            initial_session,
        } = init;

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

                Self::Input::UserSelected(selection)
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

                Self::Input::SessionSelected(selection.into())
            });

        let model = Self {
            user_selector,
            selected_user: initial_user,

            session_selector,
            selected_session: initial_session,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PreAuthViewMsg::UserSelected(selection) => self.selected_user = selection,
            PreAuthViewMsg::SessionSelected(selection) => self.selected_session = selection,

            PreAuthViewMsg::Submit => sender
                .output(Self::Output {
                    user: self.selected_user.clone(),
                    session: self.selected_session.clone(),
                })
                .expect(&format!(
                    "{S}'s Controller was dropped, cannot send messages!",
                    S = stringify!(PreAuthView),
                )),
        }
    }
}
