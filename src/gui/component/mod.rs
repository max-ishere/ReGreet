// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setup for using the greeter as a Relm4 component

use std::{collections::HashMap, fmt::Debug, path::PathBuf, process::Command};

use crate::{greetd::Greetd, sysutil::SessionInfo};
use action_button::{ActionButton, ActionButtonInit, ActionButtonOutput};
use gtk4::ContentFit;
use relm4::{gtk::prelude::*, prelude::*};

pub use auth_ui::*;
pub use greetd_controls::*;
pub use selector::*;
use tracing::{debug, error, info};

mod action_button;
mod auth_ui;
mod greetd_controls;
mod selector;

// TODO: Add a notification column component to display multiple errors. Then display different things like warnings ...
// from loading the cache files etc. This way, when there's an error, the user will see it and won't have to discover it
// through the logs.

pub struct AppInit<Client>
where
    Client: Greetd,
{
    pub users: HashMap<String, String>,
    pub sessions: HashMap<String, SessionInfo>,
    pub env: HashMap<String, String>,

    pub initial_user: String,
    pub last_user_session_cache: HashMap<String, EntryOrDropDown>,

    pub greetd_state: GreetdState<Client>,

    pub picture: Option<PathBuf>,
    pub fit: ContentFit,
    pub title_message: String,

    pub reboot_cmd: Vec<String>,
    pub poweroff_cmd: Vec<String>,
}

pub struct App<Client>
where
    Client: Greetd + 'static + Debug,
{
    reboot_cmd: Vec<String>,
    poweroff_cmd: Vec<String>,

    auth_ui: Controller<AuthUi<Client>>,

    reconnect_btn: Controller<ActionButton>,
    reboot_btn: Controller<ActionButton>,
    poweroff_btn: Controller<ActionButton>,
}

#[derive(Debug)]
pub enum AppMsg {
    Reconnect,
    Reboot,
    Poweroff,
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
                    set_valign: gtk::Align::Center,
                    inline_css: "background-color: @theme_bg_color",

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 20,
                        set_margin_all: 15,

                        append = model.reboot_btn.widget(),
                        append = model.poweroff_btn.widget(),
                        append = &gtk::Separator,
                        append = model.reconnect_btn.widget(),
                    }
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
            last_user_session_cache,
            greetd_state,
            picture,
            fit,
            title_message,
            reboot_cmd,
            poweroff_cmd,
        } = init;

        let auth_ui = AuthUi::builder()
            .launch(AuthUiInit {
                users,
                sessions,
                env,
                initial_user,
                last_user_session_cache,
                greetd_state,
            })
            .detach();

        let reconnect_btn = ActionButton::builder()
            .launch(ActionButtonInit {
                label: Some("Reload".to_string()),
                icon: "view-refresh".to_string(),
                tooltip: Some("Reconnect to greetd".to_string()),
                css_classes: vec![],
            })
            .forward(
                sender.input_sender(),
                move |ActionButtonOutput: ActionButtonOutput| AppMsg::Reconnect,
            );

        let reboot_btn = ActionButton::builder()
            .launch(ActionButtonInit {
                label: Some("Reboot".to_string()),
                icon: "system-reboot".to_string(),
                tooltip: Some("Reboot the system".to_string()),
                css_classes: vec!["destructive-action".to_string()],
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
                css_classes: vec!["destructive-action".to_string()],
            })
            .forward(
                sender.input_sender(),
                move |ActionButtonOutput: ActionButtonOutput| AppMsg::Poweroff,
            );

        let model = Self {
            reboot_cmd,
            poweroff_cmd,
            auth_ui,
            reconnect_btn,
            reboot_btn,
            poweroff_btn,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            AppMsg::Reconnect => todo!(),
            AppMsg::Reboot => exec(&self.reboot_cmd),
            AppMsg::Poweroff => exec(&self.poweroff_cmd),
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
