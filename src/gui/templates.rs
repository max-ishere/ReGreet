// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Templates for various GUI components

use gtk::prelude::*;
use relm4::{gtk, RelmWidgetExt, WidgetTemplate};

/// Button that ends the greeter (eg. Reboot)
#[relm4::widget_template(pub)]
impl WidgetTemplate for EndButton {
    view! {
        gtk::Button {
            set_focusable: true,
            add_css_class: "destructive-action",
        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for LoginButton {
    view! {
        gtk::Button {
            set_focusable: true,
            set_label: "Login",
            set_receives_default: true,
            add_css_class: "suggested-action",
        }
    }
}

/// Label for an entry/combo box
#[relm4::widget_template(pub)]
impl WidgetTemplate for EntryLabel {
    view! {
        gtk::Label {
            set_xalign: 1.0,
        }
    }
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for PaddedGrid {
    view! {
        gtk::Grid {
            set_column_spacing: 15,
            set_row_spacing: 15,
            set_margin_all: 15,
        }
    }
}

/// Main UI of the greeter
#[relm4::widget_template(pub)]
impl WidgetTemplate for Ui {
    view! {
        gtk::Overlay {
            /// Background image
            #[name = "background"]
            gtk::Picture,

            /// Main login box
            add_overlay = &gtk::Frame {
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                inline_css: "background-color: @theme_bg_color",
            },

            /// Clock widget
            add_overlay = &gtk::Frame {
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                // Make it fit cleanly onto the top edge of the screen.
                inline_css: "
                    border-top-right-radius: 0px;
                    border-top-left-radius: 0px;
                    border-top-width: 0px;
                    background-color: @theme_bg_color;
                ",

                /// Label displaying the current date & time
                #[name = "datetime_label"]
                gtk::Label { set_width_request: 150 },
            },

            /// Collection of widgets appearing at the bottom
            add_overlay = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,
                set_margin_bottom: 15,
                set_spacing: 15,

                gtk::Frame {
                    /// Notification bar for error messages
                    #[name = "error_info"]
                    gtk::InfoBar {
                        // During init, the info bar closing animation is shown. To hide that, make
                        // it invisible. Later, the code will permanently make it visible, so that
                        // `InfoBar::set_revealed` will work properly with animations.
                        set_visible: false,
                        set_message_type: gtk::MessageType::Error,

                        /// The actual error message
                        #[name = "error_label"]
                        gtk::Label {
                            set_halign: gtk::Align::Center,
                            set_margin_top: 10,
                            set_margin_bottom: 10,
                            set_margin_start: 10,
                            set_margin_end: 10,
                        },
                    }
                },

                /// Collection of buttons that close the greeter (eg. Reboot)
                gtk::Box {
                    set_halign: gtk::Align::Center,
                    set_homogeneous: true,
                    set_spacing: 15,

                    /// Button to reboot
                    #[name = "reboot_button"]
                    #[template]
                    EndButton { set_label: "Reboot" },

                    /// Button to power-off
                    #[name = "poweroff_button"]
                    #[template]
                    EndButton { set_label: "Power Off" },
                },
            },
        }
    }
}
