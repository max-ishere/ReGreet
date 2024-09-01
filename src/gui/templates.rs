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
