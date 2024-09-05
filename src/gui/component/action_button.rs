use relm4::{gtk::prelude::*, prelude::*};

pub struct ActionButtonInit {
    /// A short piece of text below the button
    pub label: Option<String>,
    /// The icon name on the button
    pub icon: String,
    /// A longer piece of text when hovering over the button
    pub tooltip: Option<String>,

    /// An optional CSS class to modify how the button looks
    pub css_classes: Vec<String>,
}

/// An icon button with a label below that can be used to perform an action.
pub struct ActionButton;

#[derive(Debug)]
pub struct ActionButtonOutput;

#[derive(Debug)]
pub struct ActionButtonMsg;

#[relm4::component(pub)]
impl SimpleComponent for ActionButton {
    type Init = ActionButtonInit;
    type Input = ActionButtonMsg;
    type Output = ActionButtonOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_tooltip_text: tooltip.as_deref(),
            set_spacing: 5,

            gtk::CenterBox {
                #[wrap(Some)]
                set_center_widget = &gtk::Button::from_icon_name(&icon) {
                    #[iterate]
                    add_css_class: css_classes,

                    set_width_request: 50,
                    set_height_request: 50,

                    connect_clicked => ActionButtonMsg,
                },
            },

            append = &gtk::Label {
                set_visible: label.is_some(),
                set_text: label.as_deref().unwrap_or_default(),
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                set_lines: 2,
                set_max_width_chars: 10,
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
            css_classes,
        } = init;

        let css_classes: Vec<&str> = css_classes.iter().map(String::as_str).collect();

        let model = Self;
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, ActionButtonMsg: Self::Input, sender: ComponentSender<Self>) {
        sender
            .output(ActionButtonOutput)
            .expect("Failed to send a message that an action button was clicked.");
    }
}
