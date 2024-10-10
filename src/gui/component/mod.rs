// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setup for using the greeter as a Relm4 component

use std::{collections::HashMap, fmt::Debug, path::PathBuf, process::Command};

use relm4::{
    gtk::{prelude::*, ContentFit},
    prelude::*,
};
use tracing::{debug, error, info};

use crate::{cache::Cache, greetd::Greetd, sysutil::SessionInfo};
use action_button::*;
use auth_ui::*;
pub use greetd_controls::GreetdState;
use greetd_controls::*;
pub use notification_item::NotificationItemInit;
use notification_list::{NotificationList, NotificationListMsg};
pub use selector::EntryOrDropDown;
use selector::*;

mod action_button;
mod auth_ui;
mod greetd_controls;
mod notification_item;
mod notification_list;
mod selector;

pub struct AppInit<Client>
where
    Client: Greetd,
{
    pub users: HashMap<String, String>,
    pub sessions: HashMap<String, SessionInfo>,
    pub env: HashMap<String, String>,

    pub initial_user: String,
    pub cache: Cache,

    pub greetd_state: GreetdState<Client>,

    pub picture: Option<PathBuf>,
    pub fit: ContentFit,
    pub title_message: String,

    pub reboot_cmd: Vec<String>,
    pub poweroff_cmd: Vec<String>,

    pub notifications: Vec<NotificationItemInit>,
}

pub struct App<Client>
where
    Client: Greetd + 'static + Debug,
{
    reboot_cmd: Vec<String>,
    poweroff_cmd: Vec<String>,

    auth_ui: Controller<AuthUi<Client>>,
    action_buttons: Vec<Controller<ActionButton>>,
    notifications: Controller<NotificationList>,
}

#[derive(Debug)]
pub enum AppMsg {
    Reboot,
    Poweroff,
    ShowNotification(NotificationItemInit),
    SessionStarted,
}

#[relm4::component(pub)]
impl<Client> SimpleComponent for App<Client>
where
    Client: Greetd + Debug,
{
    type Input = AppMsg;
    type Output = ();
    type Init = AppInit<Client>;

    view! {
        #[name = "window"]
        gtk::ApplicationWindow {
            gtk::Overlay {
                gtk::Picture {
                    set_filename: picture,
                    set_content_fit: fit,
                },

                add_overlay = &gtk::Frame {
                    set_margin_all: 15,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::End,
                    inline_css: "background-color: @theme_bg_color",

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 20,
                        set_margin_all: 15,

                        #[iterate]
                        append: model.action_buttons.iter().map(Controller::widget),
                    }
                },

                add_overlay = model.notifications.widget() {
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Fill,

                    set_margin_all: 15,

                    set_propagate_natural_width: true,
                    set_propagate_natural_height: true,
                },

                add_overlay = &gtk::Frame {
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    inline_css: "background-color: @theme_bg_color",

                    gtk::Box {
                        set_spacing: 15,
                        set_margin_all: 15,
                        set_orientation: gtk::Orientation::Vertical,

                        gtk::Label {
                            set_text: &title_message,

                            #[wrap(Some)]
                            set_attributes = &gtk::pango::AttrList {
                                insert: {
                                    let mut font_desc = gtk::pango::FontDescription::new();
                                    font_desc.set_weight(gtk::pango::Weight::Bold);
                                    gtk::pango::AttrFontDesc::new(&font_desc)
                                },
                            },
                        },

                        append = model.auth_ui.widget(),
                    }
                },
            }
        }
    }

    /// Initialize the greeter.
    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let AppInit {
            users,
            sessions,
            env,
            initial_user,
            cache,
            greetd_state,
            picture,
            fit,
            title_message,
            reboot_cmd,
            poweroff_cmd,
            notifications,
        } = init;

        let notifications = NotificationList::builder().launch(notifications).detach();

        let auth_ui = AuthUi::builder()
            .launch(AuthUiInit {
                users,
                sessions,
                env,
                initial_user,
                cache,
                greetd_state,
            })
            .forward(sender.input_sender(), |msg| match msg {
                AuthUiOutput::ShowError(error) => AppMsg::ShowNotification(NotificationItemInit {
                    markup_text: error,
                    message_type: gtk4::MessageType::Error,
                }),
                AuthUiOutput::SessionStarted => AppMsg::SessionStarted,
            });

        let reboot_btn = ActionButton::builder()
            .launch(ActionButtonInit {
                label: Some("Reboot".to_string()),
                icon: "system-reboot".to_string(),
                tooltip: Some("Reboot the system".to_string()),
                require_confirm: true,
            })
            .forward(
                sender.input_sender(),
                move |ActionButtonOutput: ActionButtonOutput| AppMsg::Reboot,
            );

        let poweroff_btn = ActionButton::builder()
            .launch(ActionButtonInit {
                label: Some("Shutdown".to_string()),
                icon: "system-shutdown".to_string(),
                tooltip: Some("Shutdown the system".to_string()),
                require_confirm: true,
            })
            .forward(
                sender.input_sender(),
                move |ActionButtonOutput: ActionButtonOutput| AppMsg::Poweroff,
            );

        let model = Self {
            reboot_cmd,
            poweroff_cmd,
            auth_ui,
            action_buttons: vec![reboot_btn, poweroff_btn],
            notifications,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        use AppMsg as I;
        match message {
            I::Reboot => exec(&self.reboot_cmd),
            I::Poweroff => exec(&self.poweroff_cmd),
            I::ShowNotification(item) => self.notifications.emit(NotificationListMsg::Notify(item)),
            I::SessionStarted => relm4::main_application().quit(),
        }
    }
}

fn exec(cmd: &[String]) {
    if cmd.is_empty() {
        debug!("Executing an empty command is a noop");
        return;
    }

    let code = Command::new(&cmd[0])
        .args(&cmd[1..])
        .status()
        .map_err(|e| error!("Error executing command {cmd:?}: {e}"));

    info!("{cmd:?} exited with code {code:?}");
}
