// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Message definitions for communication between the view and the model

use derivative::Derivative;
use greetd_ipc::Response;

use super::model::SessionSelection;

/// The messages sent by the view to the model
#[derive(Derivative)]
#[derivative(Debug)]
pub enum InputMsg {
    SessionSelected(SessionSelection),
    UserSelected(
        /// The username of the selected user
        String,
    ),

    /// Sent by the credential input to indicate an update of the credential string.
    CredentialChanged(#[derivative(Debug = "ignore")] String),

    /// Sent when the user does something that indicates they've filled out the credentia field
    /// and wanna send an auth message to greetd. Examples: enter button in text inputs, login button.
    SendAuthResp,

    /// Cancel the login request
    Cancel,

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
    MonitorRemoved(String),
}
