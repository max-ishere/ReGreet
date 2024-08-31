// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::prelude::*;
use relm4::prelude::*;

#[derive(Debug)]
pub struct SelectorInit {
    pub entry_placeholder: String,
    pub options: Vec<SelectorOption>,
    pub initial_selection: EntryOrDropDown,
    /// Whether or not this selector should startup in a locked state
    pub locked: bool,

    pub toggle_icon_name: String,
    pub toggle_tooltip: String,
}

#[derive(Debug, Clone)]
pub struct SelectorOption {
    pub id: String,
    pub text: String,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum EntryOrDropDown {
    Entry(String),
    DropDown(String),
}

#[derive(Debug, Clone)]
pub struct Selector {
    selection: EntryOrDropDown,
    locked: bool,
    last_entry: String,
    last_option_id: String,
}

#[derive(Debug)]
pub enum SelectorOutput {
    CurrentSelection(EntryOrDropDown),
}

#[derive(Debug)]
pub enum SelectorMsg {
    /// External message.
    ///
    /// Locks this input, preventing user interactions and suppressing any events it may send.
    Lock,

    /// External message.
    ///
    /// Unlocks this input, making it interactive again.
    Unlock,

    /// Internal message.
    ///
    /// Switches between the 2 input modes: [`DropDown`] and [`Entry`].
    ///
    /// [`DropDown`]: EntryOrDropDown::DropDown
    /// [`Entry`]: EntryOrDropDown::Entry
    ToggleMode,

    /// Internal message.
    ///
    /// Emited by editable fields to update the selection in the model.
    UpdateSelection(EntryOrDropDown),
}

#[relm4::component(pub)]
impl SimpleComponent for Selector {
    type Init = SelectorInit;
    type Input = SelectorMsg;
    type Output = SelectorOutput;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 15,

            #[transition = "SlideLeftRight"]
            append = match &model.selection {

                EntryOrDropDown::DropDown(_) => {
                    #[name = "combo_box"]
                    gtk::ComboBoxText {
                        set_hexpand: true,

                        // Note: Cannot `set_active_id` here, because the options are appended after `view_output!{}` in `init`

                        #[watch]
                        set_sensitive: !model.locked,
                        connect_changed[sender] => move |dropdown| {
                            if !dropdown.is_sensitive() {
                                return;
                            }

                            sender.input(
                                Self::Input::UpdateSelection(
                                    EntryOrDropDown::DropDown(dropdown.active_id().unwrap().to_string())
                                )
                            )
                        }
                    }
                }

                EntryOrDropDown::Entry(_) => {
                    gtk::Entry {
                        set_hexpand: true,
                        // Note: not `#[watch] model.selection.text` because `set_text()` places the cursor at char 0.
                        set_text: model.last_entry.as_str(),
                        set_placeholder_text: Some(entry_placeholder.as_str()),

                        #[watch]
                        set_sensitive: !model.locked,
                        connect_changed[sender] => move |entry| {
                            if !entry.is_sensitive() {
                                return;
                            }

                            sender.input(
                                Self::Input::UpdateSelection(
                                    EntryOrDropDown::Entry(entry.text().to_string())
                                )
                            )
                        }
                    }
                }
            },

            append = &gtk::ToggleButton {
                set_icon_name: toggle_icon_name.as_str(),
                set_active: toggle_state,
                set_tooltip_text: Some(toggle_tooltip.as_str()),


                #[watch]
                set_sensitive: !model.locked,
                connect_clicked[sender] => move |toggle| {
                    if !toggle.is_sensitive() {
                        return;
                    }

                    sender.input(Self::Input::ToggleMode)
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let SelectorInit {
            options,
            initial_selection: selection,
            locked,
            toggle_icon_name,
            toggle_tooltip,
            entry_placeholder,
        } = init;

        let (last_entry, last_option_id, toggle_state) = match &selection {
            EntryOrDropDown::Entry(entry) => (entry.clone(), options[0].id.clone(), true),
            EntryOrDropDown::DropDown(id) => (String::new(), id.clone(), false),
        };

        let model = Self {
            selection,
            locked,

            last_entry,
            last_option_id,
        };

        let widgets = view_output!();

        // #[iterate] doesn't support a way to provide 2 iterators, thus have to populate combo box manually
        options
            .iter()
            .for_each(|opt| widgets.combo_box.append(Some(&opt.id), &opt.text));

        // TODO: Figure out if `update` should be supressed.

        let id_comes_from_options = widgets.combo_box.set_active_id(Some(&model.last_option_id));

        if !id_comes_from_options {
            unreachable!(
                "The id `{id}` must be from the options list, all of which must be inserted before the active default is set.",
                id = model.last_option_id,
            )
        }

        // Because `set_active_id` emits an update model signal
        if let EntryOrDropDown::Entry(_) = model.selection {
            sender.input(SelectorMsg::ToggleMode);
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        use SelectorMsg as I;

        match message {
            I::ToggleMode => {
                let new = match &self.selection {
                    EntryOrDropDown::Entry(last) => {
                        self.last_entry = last.clone();
                        EntryOrDropDown::DropDown(self.last_option_id.clone())
                    }
                    EntryOrDropDown::DropDown(last) => {
                        self.last_option_id = last.clone();
                        EntryOrDropDown::Entry(self.last_entry.clone())
                    }
                };

                self.selection = new.clone();
                sender.output(Self::Output::CurrentSelection(new)).expect(
                    "selector's controller must not be dropped because this is an input widget.",
                )
            }

            I::UpdateSelection(new) => {
                self.selection = new.clone();
                sender.output(Self::Output::CurrentSelection(new)).expect(
                    "selector's controller must not be dropped because this is an input widget.",
                )
            }

            I::Lock => self.locked = true,
            I::Unlock => self.locked = false,
        }
    }
}
