use relm4::{factory::FactoryVecDeque, gtk::prelude::*, prelude::*};

use super::notification_item::{NotificationItem, NotificationItemInit};

pub struct NotificationList {
    items: FactoryVecDeque<NotificationItem>,
}

#[derive(Debug)]
pub enum NotificationListMsg {
    /// Internal message
    ///
    /// Dismisses a notification at this index. Emited by the notification item.
    Dismiss(DynamicIndex),

    /// External message.
    ///
    /// Show a nofication.
    Notify(NotificationItemInit),
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationList {
    type Init = Vec<NotificationItemInit>;
    type Input = NotificationListMsg;
    type Output = ();

    view! {
        gtk::ScrolledWindow {
            #[local_ref]
            items -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 15,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut items = FactoryVecDeque::new(gtk::Box::default(), sender.input_sender());
        let _ = init.into_iter().fold(items.guard(), |mut guard, item| {
            guard.push_back(item);
            guard
        });

        let model = Self { items };

        let items = model.items.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        use NotificationListMsg as I;
        match message {
            I::Dismiss(index) => {
                self.items.guard().remove(index.current_index());
            }
            I::Notify(init) => {
                self.items.guard().push_back(init);
            }
        }
    }
}
