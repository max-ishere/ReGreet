use super::notification_list::NotificationListMsg;
use relm4::prelude::*;

#[derive(Debug)]
pub struct NotificationItemInit {
    pub markup_text: String,
    pub message_type: gtk::MessageType,
}
pub struct NotificationItem(NotificationItemInit);

#[derive(Debug)]
pub enum NotificationItemOutput {
    Dismissed(DynamicIndex),
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationItem {
    type Init = NotificationItemInit;
    type Input = ();
    type Output = NotificationItemOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type ParentInput = NotificationListMsg;

    view! {
        #[root]
        gtk::Frame {
            gtk::InfoBar {
                set_show_close_button: true,
                set_message_type: self.0.message_type,

                connect_response[sender, index] => move |_,_| {
                    sender.output(NotificationItemOutput::Dismissed(index.clone()));
                },

                gtk::Label {
                    set_max_width_chars: 30,
                    set_width_chars: 30,
                    set_wrap: true,
                    set_markup: &self.0.markup_text,
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self(init)
    }

    fn output_to_parent_input(
        NotificationItemOutput::Dismissed(index): Self::Output,
    ) -> Option<Self::ParentInput> {
        Some(NotificationListMsg::Dismiss(index))
    }
}
