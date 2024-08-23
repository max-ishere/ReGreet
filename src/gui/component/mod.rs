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
use super::model::{Greeter, InputMode, Updates};
use super::templates::Ui;

mod selector;
pub use selector::*;

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
        // The `view!` macro needs a proper widget, not a template, as the root.
        #[name = "window"]
        gtk::ApplicationWindow {
            set_visible: true,

            // Name the UI widget, otherwise the inner children cannot be accessed by name.
            #[name = "ui"]
            #[template]
            Ui {
                #[template_child] grid {
                    // TODO: Split the grid into 2 separate templates and conditionally reveal one or the other on the .is_input()
                    attach[1, 2, 2, 1] = if !model.updates.is_input() {
                        model.session_selector.widget().clone() {
                        }
                    } else {
                        // TODO: THIS BOX PREVENTS TOUCH EVENTS FROM GOING TO THE PASSWORD FIELD, REMOVE IT
                        gtk::Box {}
                    },

                    // TODO: Lock this widget after session is created
                    attach[1, 1, 2, 1] = model.user_selector.widget() {
                    },
                },

                #[template_child]
                background { set_filename: model.config.get_background().clone() },
                #[template_child]
                datetime_label {
                    #[track(model.updates.changed(Updates::time()))]
                    set_label: &model.updates.time
                },

                #[template_child]
                message_label {
                    #[track(model.updates.changed(Updates::message()))]
                    set_label: &model.updates.message,
                },
                #[template_child]
                session_label {
                    #[track(model.updates.changed(Updates::input_mode()))]
                    set_visible: !model.updates.is_input(),
                },
                #[template_child]
                input_label {
                    #[track(model.updates.changed(Updates::input_mode()))]
                    set_visible: model.updates.is_input(),
                    #[track(model.updates.changed(Updates::input_prompt()))]
                    set_label: &model.updates.input_prompt,
                },
                #[template_child]
                secret_entry {
                    #[track(model.updates.changed(Updates::input_mode()))]
                    set_visible: model.updates.input_mode == InputMode::Secret,

                    #[track(
                        model.updates.changed(Updates::input_mode())
                        && model.updates.input_mode == InputMode::Secret
                    )]
                    grab_focus: (),

                    #[track(model.updates.changed(Updates::input()))]
                    set_text: &model.updates.input,

                    connect_changed[sender] => move |this| {
                        sender.input(Self::Input::CredentialChanged(this.text().to_string()))
                    },

                    connect_activate => Self::Input::SendAuthResp,
                },
                #[template_child]
                visible_entry {
                    #[track(model.updates.changed(Updates::input_mode()))]
                    set_visible: model.updates.input_mode == InputMode::Visible,

                    #[track(
                        model.updates.changed(Updates::input_mode())
                        && model.updates.input_mode == InputMode::Visible
                    )]
                    grab_focus: (),

                    #[track(model.updates.changed(Updates::input()))]
                    set_text: &model.updates.input,

                    connect_changed[sender] => move |this| {
                        sender.input(Self::Input::CredentialChanged(this.text().to_string()))
                    },

                    connect_activate => Self::Input::SendAuthResp,
                },
                #[template_child]
                cancel_button {
                    #[track(model.updates.changed(Updates::input_mode()))]
                    set_visible: model.updates.is_input(),
                    connect_clicked => Self::Input::Cancel,
                },
                #[template_child]
                login_button {
                    #[track(
                        model.updates.changed(Updates::input_mode())
                        && !model.updates.is_input()
                    )]
                    grab_focus: (),

                    connect_clicked => Self::Input::SendAuthResp,
                },
                #[template_child]
                error_info {
                    #[track(model.updates.changed(Updates::error()))]
                    set_revealed: model.updates.error.is_some(),
                },
                #[template_child]
                error_label {
                    #[track(model.updates.changed(Updates::error()))]
                    set_label: model.updates.error.as_ref().unwrap_or(&"".to_string()),
                },
                #[template_child]
                reboot_button { connect_clicked => Self::Input::Reboot },
                #[template_child]
                poweroff_button { connect_clicked => Self::Input::PowerOff },
            }
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
        let mut model = Self::new(&input.config_path, input.demo, &sender).await;

        let widgets = view_output!();

        // Make the info bar permanently visible, since it was made invisible during init. The
        // actual visuals are controlled by `InfoBar::set_revealed`.
        widgets.ui.error_info.set_visible(true);

        // cfg directives don't work inside Relm4 view! macro.
        #[cfg(feature = "gtk4_8")]
        widgets
            .ui
            .background
            .set_content_fit(match model.config.get_background_fit() {
                BgFit::Fill => gtk4::ContentFit::Fill,
                BgFit::Contain => gtk4::ContentFit::Contain,
                BgFit::Cover => gtk4::ContentFit::Cover,
                BgFit::ScaleDown => gtk4::ContentFit::ScaleDown,
            });

        // Cancel any previous session, just in case someone started one.
        if let Err(err) = model.greetd_client.lock().await.cancel_session().await {
            warn!("Couldn't cancel greetd session: {err}");
        };

        model.choose_monitor(widgets.ui.display().name().as_str(), &sender);
        if let Some(monitor) = &model.updates.monitor {
            // The window needs to be manually fullscreened, since the monitor is `None` at widget
            // init.
            root.fullscreen_on_monitor(monitor);
        } else {
            // Couldn't choose a monitor, so let the compositor choose it for us.
            root.fullscreen();
        }

        // For some reason, the GTK settings are reset when changing monitors, so apply them after
        // full-screening.
        setup_settings(&model, &root);
        setup_datetime_display(&sender);

        if input.css_path.exists() {
            debug!("Loading custom CSS from file: {}", input.css_path.display());
            let provider = gtk::CssProvider::new();
            provider.load_from_path(input.css_path);
            gtk::StyleContext::add_provider_for_display(
                &widgets.ui.display(),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        };

        // Set the default behaviour of pressing the Return key to act like the login button.
        root.set_default_widget(Some(&widgets.ui.login_button));

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
            Self::Input::UserSelected(new) => self.selected_user = new,
            Self::Input::SessionSelected(str) => {
                self.selected_session = str;
            }

            Self::Input::CredentialChanged(new) => self.credential = new,
            Self::Input::SendAuthResp => self.login_click_handler(&sender).await,

            Self::Input::Cancel => self.cancel_click_handler().await,

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
                self.handle_greetd_response(&sender, response).await
            }
            Self::CommandOutput::MonitorRemoved(display_name) => {
                self.choose_monitor(display_name.as_str(), &sender)
            }
        };
    }
}
