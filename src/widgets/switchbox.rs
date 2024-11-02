use relm4::{FactorySender, RelmWidgetExt, Sender};
use relm4::prelude::{DynamicIndex, FactoryComponent};
use relm4::factory::{FactoryVecDeque, FactoryView};

use gtk::{Align, Orientation};
use gtk::glib::{self, Object};
use gtk::prelude::{BoxExt, GestureSingleExt, OrientableExt, WidgetExt};

use crate::app::Message;
use crate::server::Output;

#[derive(Clone, Debug)]
pub enum SwitchMessage {
    Activate,
    Deactivate,
    Click,
}

pub struct Switches {
    pub container: FactoryVecDeque<Switch>,
}

impl Switches {
    pub fn new(sender: &Sender<Message>) -> Self {
        let container = FactoryVecDeque::builder()
            .launch(SwitchBox::default())
            .forward(sender, std::convert::identity);

        Self { container }
    }

    pub fn push(&mut self, output: Output) {
        let mut switches = self.container.guard();

        switches.push_front(output);
        switches.drop();
    }

    fn position(&self, output: Output) -> Option<usize> {
        self.container.iter().position(|switch| switch.name == output.name && switch.port == output.port)
    }

    pub fn set_active(&self, output: Output) {
        self.container.broadcast(SwitchMessage::Deactivate);

        if let Some(pos) = self.position(output) {
            self.container.send(pos, SwitchMessage::Activate);
        }

    }
}

#[tracker::track]
pub struct Switch {
    name:   String,
    port:   String,
    active: bool,
}

#[relm4::factory(pub)]
impl FactoryComponent for Switch {
    type Init = Output;
    type Input = SwitchMessage;
    type Output = Message;
    type ParentWidget = SwitchBox;
    type CommandOutput = ();

    view! {
        root = gtk::Box {
            add_css_class: "output",
            set_orientation: Orientation::Horizontal,

            #[track = "self.changed(Self::active())"]
            set_class_active: ("master", self.active),

            add_controller = gtk::GestureClick {
                set_button: gtk::gdk::BUTTON_PRIMARY,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input_sender().emit(SwitchMessage::Click)
                }
            },

            gtk::Image {
                set_expand: true,
                set_align: Align::Center,
                add_css_class: "icon",
                set_use_fallback: false,
                #[track = "self.changed(Self::port())"]
                set_icon_name: Some(icon(&self.port)),
            },
        }
    }

    fn init_widgets(&mut self, _: &Self::Index, root: Self::Root, _: &<Self::ParentWidget as FactoryView>::ReturnedWidget, sender: FactorySender<Self>) -> Self::Widgets {
        let widgets = view_output!();

        widgets
    }

    fn init_model(init: Self::Init, _: &DynamicIndex, _: FactorySender<Self>) -> Self {
        Self {
            name: init.name,
            port: init.port,
            active: init.master,

            tracker: 0,
        }
    }

    fn update_cmd(&mut self, _: Self::CommandOutput, _: FactorySender<Self>) {
        self.reset();
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        self.reset();

        match message {
            SwitchMessage::Activate => self.set_active(true),
            SwitchMessage::Deactivate => self.set_active(false),
            SwitchMessage::Click => sender.output_sender().emit(Message::SetOutput {
                name: self.name.as_str().into(),
                port: self.port.as_str().into(),
            })
        }
    }
}

fn icon(s: &str) -> &'static str {
    let s = s.to_ascii_lowercase();

    if s.contains("headphones") {
        "audio-headphones-symbolic"
    }
    else if s.contains("hdmi") {
        "computer-symbolic"
    }
    else {
        "multimedia-player-symbolic"
    }
}

mod imp {
    use std::cell::Cell;

    use glib::Properties;
    use glib::subclass::types::ObjectSubclass;
    use glib::subclass::object::ObjectImpl;

    use gtk::prelude::ObjectExt;
    use gtk::subclass::box_::BoxImpl;
    use gtk::subclass::widget::WidgetImpl;
    use gtk::subclass::orientable::OrientableImpl;
    use gtk::subclass::prelude::DerivedObjectProperties;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::SwitchBox)]
    pub struct SwitchBox {
        #[property(get, set)]
        active: Cell<f64>,
    }

    impl WidgetImpl for SwitchBox {}
    impl OrientableImpl for SwitchBox {}
    impl BoxImpl for SwitchBox {}

    #[glib::object_subclass]
    impl ObjectSubclass for SwitchBox {
        const NAME: &'static str = "SwitchBox";
        type Type = super::SwitchBox;
        type ParentType = gtk::Box;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SwitchBox {}
}

glib::wrapper! {
    pub struct SwitchBox(ObjectSubclass<imp::SwitchBox>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SwitchBox {
    pub fn default() -> Self {
        Object::builder().build()
    }
}

// FactoryComponent::ParentWidget must implement FactoryView.
// This is the same implementation that is used for gtk::Box.
// https://docs.rs/relm4/0.7.0-rc.1/src/relm4/factory/widgets/gtk.rs.html#5
impl FactoryView for SwitchBox {
    type Children = gtk::Widget;
    type ReturnedWidget = gtk::Widget;
    type Position = ();

    fn factory_remove(&self, widget: &Self::ReturnedWidget) {
        self.remove(widget);
    }

    fn factory_append(&self, widget: impl AsRef<Self::Children>, _: &Self::Position) -> Self::ReturnedWidget {
        self.append(widget.as_ref());
        widget.as_ref().clone()
    }

    fn factory_prepend(&self, widget: impl AsRef<Self::Children>, _: &Self::Position) -> Self::ReturnedWidget {
        self.prepend(widget.as_ref());
        widget.as_ref().clone()
    }

    fn factory_insert_after(&self, widget: impl AsRef<Self::Children>, _: &Self::Position, other: &Self::ReturnedWidget) -> Self::ReturnedWidget {
        self.insert_child_after(widget.as_ref(), Some(other));
        widget.as_ref().clone()
    }

    fn returned_widget_to_child(returned_widget: &Self::ReturnedWidget) -> Self::Children {
        returned_widget.clone()
    }

    fn factory_move_after(&self, widget: &Self::ReturnedWidget, other: &Self::ReturnedWidget) {
        self.reorder_child_after(widget, Some(other));
    }

    fn factory_move_start(&self, widget: &Self::ReturnedWidget) {
        self.reorder_child_after(widget, None::<&gtk::Widget>);
    }
}
