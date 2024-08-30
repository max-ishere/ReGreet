// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setup for using the greeter as a Relm4 component

use std::path::PathBuf;
use std::time::Duration;

use chrono::Local;
use tracing::{debug, warn};

use gtk4::prelude::*;
use relm4::{
    component::{AsyncComponent, AsyncComponentParts, AsyncComponentSender},
    prelude::*,
};
use tokio::time::sleep;

#[cfg(feature = "gtk4_8")]
use crate::config::BgFit;

use super::messages::{CommandMsg, InputMsg};
use super::model::{Greeter, Updates};

mod selector;
pub use selector::*;

mod auth_ui;
pub use auth_ui::*;

mod auth_view;
pub use auth_view::*;

const DATETIME_FMT: &str = "%a %R";
const DATETIME_UPDATE_DELAY: u64 = 500;

/// Load GTK settings from the greeter config.
fn setup_settings(model: &Greeter, root: &gtk::ApplicationWindow) {
    let settings = root.settings();
    let config = if let Some(config) = model.config.get_gtk_settings() {
        config
    } else {
        return;
    };

    debug!(
        "Setting dark theme: {}",
        config.application_prefer_dark_theme
    );
    settings.set_gtk_application_prefer_dark_theme(config.application_prefer_dark_theme);

    if let Some(cursor_theme) = &config.cursor_theme_name {
        debug!("Setting cursor theme: {cursor_theme}");
        settings.set_gtk_cursor_theme_name(config.cursor_theme_name.as_deref());
    };

    if let Some(font) = &config.font_name {
        debug!("Setting font: {font}");
        settings.set_gtk_font_name(config.font_name.as_deref());
    };

    if let Some(icon_theme) = &config.icon_theme_name {
        debug!("Setting icon theme: {icon_theme}");
        settings.set_gtk_icon_theme_name(config.icon_theme_name.as_deref());
    };

    if let Some(theme) = &config.theme_name {
        debug!("Setting theme: {theme}");
        settings.set_gtk_theme_name(config.theme_name.as_deref());
    };
}

/// Set up auto updation for the datetime label.
fn setup_datetime_display(sender: &AsyncComponentSender<Greeter>) {
    // Set a timer in a separate thread that signals the main thread to update the time, so as to
    // not block the GUI.
    sender.command(|sender, shutdown| {
        shutdown
            .register(async move {
                // Run it infinitely, since the clock always needs to stay updated.
                loop {
                    if sender.send(CommandMsg::UpdateTime).is_err() {
                        warn!("Couldn't update datetime");
                    };
                    sleep(Duration::from_millis(DATETIME_UPDATE_DELAY)).await;
                }
            })
            .drop_on_shutdown()
    });
}

/// The info required to initialize the greeter
pub struct GreeterInit {
    pub config_path: PathBuf,
    pub css_path: PathBuf,
    pub demo: bool,
}

#[relm4::component(pub, async)]
impl AsyncComponent for Greeter {
    type Input = InputMsg;
    type Output = ();
    type Init = GreeterInit;
    type CommandOutput = CommandMsg;

    // UI flow:
    //
    // Open user/session screen
    //
    // when login, create session and unless can start session (in which case start it) go to auth page
    //
    // On auth page both user and session widget active. If session widget changed, update model,
    // if user widget changed, cancel this session, start new session with that user. However, no
    // type input. For type input, go to main screen. Or make it so that there's pressing enter required
    // to confirm username and display this enter requirement to the user. But at this point, it's just
    // the main screen.
    //
    // TODO: Also add an (i) hint thing to the second screen where if you hover over it or click ...
    // or some, it tells you that this dropdown switches to a different user.
    //
    // At this point what is even the point of having a second screen? Oh, its to prevent PAM and greetd timeouts.

    view! {
        #[name = "window"]
        gtk::ApplicationWindow {
            model.auth_ui.widget(),
        }
    }

    fn post_view() {
        if model.updates.changed(Updates::monitor()) {
            if let Some(monitor) = &model.updates.monitor {
                widgets.window.fullscreen_on_monitor(monitor);
                // For some reason, the GTK settings are reset when changing monitors, so re-apply them.
                setup_settings(self, &widgets.window);
            }
        }
    }

    /// Initialize the greeter.
    async fn init(
        input: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = Self::new(&input.config_path, input.demo, &sender).await;

        let widgets = view_output!();

        // For some reason, the GTK settings are reset when changing monitors, so apply them after
        // full-screening.
        setup_settings(&model, &root);
        setup_datetime_display(&sender);

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        debug!("Got input message: {msg:?}");

        // Reset the tracker for update changes.
        self.updates.reset();

        match msg {
            Self::Input::Reboot => self.reboot_click_handler(&sender),
            Self::Input::PowerOff => self.poweroff_click_handler(&sender),
        }
    }

    /// Perform the requested changes when a background task sends a message.
    async fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        if !matches!(msg, Self::CommandOutput::UpdateTime) {
            debug!("Got command message: {msg:?}");
        }

        // Reset the tracker for update changes.
        self.updates.reset();

        match msg {
            Self::CommandOutput::UpdateTime => self
                .updates
                .set_time(Local::now().format(DATETIME_FMT).to_string()),
            Self::CommandOutput::ClearErr => self.updates.set_error(None),
            Self::CommandOutput::HandleGreetdResponse(response) => {
                // self.handle_greetd_response(&sender, response).await
            }
            Self::CommandOutput::MonitorRemoved(display_name) => {
                self.choose_monitor(display_name.as_str(), &sender)
            }
        };
    }
}
