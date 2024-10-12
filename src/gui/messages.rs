// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Message definitions for communication between the view and the model

use derivative::Derivative;
use greetd_ipc::Response;
use relm4::gtk::{glib::GString, prelude::*, ComboBoxText, Entry};

use super::model::SessionSelection;

#[derive(Debug)]
/// Info about the current user and chosen session
pub struct UserInfo {
    /// The ID for the currently chosen user
    pub(super) user_id: Option<GString>,
    /// The entry text for the currently chosen user
    pub(super) user_text: GString,
}

impl UserInfo {
    /// Extract session and user info from the relevant widgets.
    pub(super) fn extract(usernames_box: &ComboBoxText, username_entry: &Entry) -> Self {
        Self {
            user_id: usernames_box.active_id(),
            user_text: username_entry.text(),
        }
    }
}

/// The messages sent by the view to the model
#[derive(Derivative)]
#[derivative(Debug)]
pub enum InputMsg {
    /// Login request
    Login {
        #[derivative(Debug = "ignore")]
        input: String,
        info: UserInfo,
    },
    /// Cancel the login request
    Cancel,
    /// The current user was changed in the GUI.
    UserChanged(UserInfo),
    SessionSelected(SessionSelection),
    /// Toggle manual entry of user.
    ToggleManualUser,
    Reboot,
    PowerOff,
}

#[derive(Debug)]
/// The messages sent to the sender to run tasks in the background
pub enum CommandMsg {
    /// Update the clock.
    UpdateTime,
    /// Clear the error message.
    ClearErr,
    /// Handle a response received from greetd
    HandleGreetdResponse(Response),
    /// Notify the greeter that a monitor was removed.
    // The Gstring is the name of the display.
    MonitorRemoved(GString),
}
