// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

// SPDX-FileCopyrightText: 2021 Maximilian Moser <maximilian.moser@tuwien.ac.at>
//
// SPDX-License-Identifier: MIT

//! The main logic for the greeter

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gtk4::{
    gdk::{Display, Monitor},
    prelude::*,
};
use relm4::{prelude::*, AsyncComponentSender};
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, warn};

use crate::cache::Cache;
use crate::config::Config;
use crate::greetd::MockGreetd;
use crate::sysutil::SysUtil;

use super::{
    component::{AuthUi, AuthUiInit, EntryOrDropDown, GreetdState, SelectorOption},
    messages::CommandMsg,
};

const ERROR_MSG_CLEAR_DELAY: u64 = 5;

#[derive(PartialEq)]
pub(super) enum InputMode {
    None,
    Secret,
    Visible,
}

// Fields only set by the model, that are meant to be read only by the widgets
#[tracker::track]
pub(super) struct Updates {
    /// Message to be shown to the user
    pub(super) message: String,
    /// Error message to be shown to the user below the prompt
    pub(super) error: Option<String>,
    /// Text in the password field
    pub(super) input: String,
    /// Whether the username is being entered manually
    pub(super) manual_user_mode: bool,
    /// Input prompt sent by greetd for text input
    pub(super) input_prompt: String,
    /// Whether the user is currently entering a secret, something visible or nothing
    pub(super) input_mode: InputMode,
    /// ID of the active session
    pub(super) active_session_id: Option<String>,
    /// Time that is displayed
    pub(super) time: String,
    /// Monitor where the window is displayed
    pub(super) monitor: Option<Monitor>,
}

impl Updates {
    pub(super) fn is_input(&self) -> bool {
        self.input_mode != InputMode::None
    }
}

/// Greeter model that holds its state
pub struct Greeter {
    /// The cache that persists between logins
    pub(super) cache: Cache,
    /// The config for this greeter
    pub(super) config: Config,
    /// The updates from the model that are read by the view
    pub(super) updates: Updates,
    /// Is it run as demo
    pub(super) demo: bool,

    pub(super) auth_ui: Controller<AuthUi<MockGreetd>>,
}

impl Greeter {
    pub(super) async fn new(config_path: &Path, demo: bool) -> Self {
        let sys_util = SysUtil::new().expect("Couldn't read available users and sessions");

        let users: Vec<_> = sys_util
            .get_users()
            .iter()
            .map(|(fullname, username)| SelectorOption {
                id: username.clone(),
                text: fullname.clone(),
            })
            .collect();

        let initial_user = EntryOrDropDown::DropDown(users[0].id.clone());

        let sessions: Vec<_> = sys_util
            .get_sessions()
            .keys()
            .map(|name| SelectorOption {
                id: name.clone(),
                text: name.clone(),
            })
            .collect();

        let initial_session = EntryOrDropDown::DropDown(sessions[0].id.clone());

        let auth_ui = AuthUi::builder()
            .launch(AuthUiInit {
                // TODO: Move GreeterInit
                sys_util,
                users,
                initial_user: initial_user.clone(),
                sessions,
                initial_session,
                greetd_state: GreetdState::NotStarted(MockGreetd {}),
            })
            .detach();

        let config = Config::new(config_path);

        let updates = Updates {
            message: config.get_default_message(),
            error: None,
            input: String::new(),
            manual_user_mode: false,
            input_mode: InputMode::None,
            input_prompt: String::new(),
            active_session_id: None,
            time: "".to_string(),
            monitor: None,

            tracker: 0,
        };
        Self {
            cache: Cache::new(),
            config,
            updates,
            demo,
            auth_ui,
        }
    }

    /// Make the greeter full screen over the first monitor.
    #[instrument(skip(self, sender))]
    pub(super) fn choose_monitor(
        &mut self,
        display_name: &str,
        sender: &AsyncComponentSender<Self>,
    ) {
        let display = match Display::open(display_name) {
            Some(display) => display,
            None => {
                error!("Couldn't get display with name: {display_name}");
                return;
            }
        };

        let mut chosen_monitor = None;
        for monitor in display
            .monitors()
            .into_iter()
            .filter_map(|item| {
                item.ok()
                    .and_then(|object| object.downcast::<Monitor>().ok())
            })
            .filter(Monitor::is_valid)
        {
            debug!("Found monitor: {monitor}");
            let sender = sender.clone();
            monitor.connect_invalidate(move |monitor| {
                let display_name = monitor.display().name();
                sender.oneshot_command(async move {
                    CommandMsg::MonitorRemoved(display_name.to_string())
                })
            });
            if chosen_monitor.is_none() {
                // Choose the first monitor.
                chosen_monitor = Some(monitor);
            }
        }

        self.updates.set_monitor(chosen_monitor);
    }

    /// Run a command and log any errors in a background thread.
    fn run_cmd(command: &[String], sender: &AsyncComponentSender<Self>) {
        let mut process = Command::new(&command[0]);
        process.args(command[1..].iter());
        // Run the command and check its output in a separate thread, so as to not block the GUI.
        sender.spawn_command(move |_| match process.output() {
            Ok(output) => {
                if !output.status.success() {
                    if let Ok(err) = std::str::from_utf8(&output.stderr) {
                        error!("Failed to launch command: {err}")
                    } else {
                        error!("Failed to launch command: {:?}", output.stderr)
                    }
                }
            }
            Err(err) => error!("Failed to launch command: {err}"),
        });
    }

    /// Event handler for clicking the "Reboot" button
    ///
    /// This reboots the PC.
    #[instrument(skip_all)]
    pub(super) fn reboot_click_handler(&self, sender: &AsyncComponentSender<Self>) {
        if self.demo {
            info!("demo: skip reboot");
            return;
        }
        info!("Rebooting");
        Self::run_cmd(&self.config.get_sys_commands().reboot, sender);
    }

    /// Event handler for clicking the "Power-Off" button
    ///
    /// This shuts down the PC.
    #[instrument(skip_all)]
    pub(super) fn poweroff_click_handler(&self, sender: &AsyncComponentSender<Self>) {
        if self.demo {
            info!("demo: skip shutdown");
            return;
        }
        info!("Shutting down");
        Self::run_cmd(&self.config.get_sys_commands().poweroff, sender);
    }

    /// Show an error message to the user.
    fn display_error(
        &mut self,
        sender: &AsyncComponentSender<Self>,
        display_text: &str,
        log_text: &str,
    ) {
        self.updates.set_error(Some(display_text.to_string()));
        error!("{log_text}");

        sender.oneshot_command(async move {
            sleep(Duration::from_secs(ERROR_MSG_CLEAR_DELAY)).await;
            CommandMsg::ClearErr
        });
    }
}
