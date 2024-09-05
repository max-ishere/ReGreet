// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setup for using the greeter as a Relm4 component

use std::{collections::HashMap, path::PathBuf};

use gtk4::ContentFit;
use relm4::{gtk::prelude::*, prelude::*};

#[cfg(feature = "gtk4_8")]
use crate::config::BgFit;
use crate::{greetd::Greetd, sysutil::SessionInfo};

mod selector;
pub use selector::*;

mod auth_ui;
pub use auth_ui::*;

mod greetd_controls;
pub use greetd_controls::*;

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
}

pub struct App<Client>
where
    Client: Greetd + 'static,
{
    auth_ui: Controller<AuthUi<Client>>,
}

#[relm4::component(pub)]
impl<Client> SimpleComponent for App<Client>
where
    Client: Greetd,
{
    type Input = ();
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
        _sender: ComponentSender<Self>,
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

        let model = Self { auth_ui };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}
