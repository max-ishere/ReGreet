use relm4::{gtk::prelude::*, prelude::*};

pub struct ActionButtonInit {
    /// A short piece of text below the button
    pub label: Option<String>,

    /// The icon name on the button
    pub icon: String,

    /// A longer piece of text when hovering over the button
    pub tooltip: Option<String>,

    /// If [`true`], then the button will ask for a confirmation before the action message is sent.
    pub require_confirm: bool,
}

/// An icon button with a label below that can be used to perform an action.
pub struct ActionButton {
    state: ActionButtonState,
    require_confirm: bool,
}

pub enum ActionButtonState {
    ConfirmTheAction,
    Normal,
}
#[derive(Debug)]
pub struct ActionButtonOutput;

#[derive(Debug)]
pub enum ActionButtonMsg {
    Unconfirmed,
    Confirmation,
    Cancel,
}

#[relm4::component(pub)]
impl SimpleComponent for ActionButton {
    type Init = ActionButtonInit;
    type Input = ActionButtonMsg;
    type Output = ActionButtonOutput;

    view! {
        gtk::Box {
            set_tooltip_text: tooltip.as_deref(),
            set_spacing: 15,

            #[transition = "SlideLeftRight"]
            match model.state {
                ActionButtonState::ConfirmTheAction => gtk::Button::from_icon_name("window-close-symbolic") {
                    #[watch]
                    grab_focus: (),

                    connect_clicked => ActionButtonMsg::Cancel,
                },

                ActionButtonState::Normal => gtk::Button::from_icon_name(&icon) {
                    #[iterate]
                    add_css_class: danger_class.clone(),

                    connect_clicked => ActionButtonMsg::Unconfirmed,
                }
            },

            gtk::Label {
                set_visible: label.is_some(),
                set_text: label.as_deref().unwrap_or_default(),
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                set_max_width_chars: 10,
            },

            gtk::Revealer {
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                #[watch]
                set_reveal_child: matches!(model.state, ActionButtonState::ConfirmTheAction),

                gtk::Button::from_icon_name(&icon) {
                    #[iterate]
                    add_css_class: danger_class.clone(),

                    connect_clicked => ActionButtonMsg::Confirmation,
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let ActionButtonInit {
            label,
            icon,
            tooltip,
            require_confirm,
        } = init;

        let danger_class = require_confirm.then_some("destructive-action").into_iter();

        let model = Self {
            require_confirm,
            state: ActionButtonState::Normal,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        use ActionButtonMsg as I;
        use ActionButtonState as S;
        match (message, &self.state, self.require_confirm) {
            (I::Cancel, _, _) => self.state = S::Normal,

            (I::Unconfirmed | I::Confirmation, S::Normal, true) => self.state = S::ConfirmTheAction,

            (I::Confirmation, S::ConfirmTheAction, true)
            | (I::Unconfirmed | I::Confirmation, _, false) => {
                self.state = S::Normal;
                sender.output(ActionButtonOutput).unwrap();
            }

            (I::Unconfirmed, S::ConfirmTheAction, true) => (),
        }
    }
}
