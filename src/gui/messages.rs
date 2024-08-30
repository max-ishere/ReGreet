// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Message definitions for communication between the view and the model

use derivative::Derivative;
use greetd_ipc::Response;

use super::component::EntryOrDropDown;

/// The messages sent by the view to the model
#[derive(Derivative)]
#[derivative(Debug)]
pub enum InputMsg {
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
