// This is a 99% GTK boilerplate to get custom Widget.
// At this moment it just adds a single property to the Box.
// https://gtk-rs.org/gtk4-rs/git/book/g_object_subclassing.html
mod imp;

use relm4::gtk;
use relm4::factory::FactoryView;

use gtk::glib::{self, Object};
use gtk::prelude::BoxExt;

glib::wrapper! {
    pub struct SliderBox(ObjectSubclass<imp::SliderBox>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SliderBox {
    pub fn default() -> Self {
        Object::builder().build()
    }
}

// FactoryComponent::ParentWidget must implement FactoryView.
// This is the same implementation that is used for gtk::Box.
// https://docs.rs/relm4/0.7.0-rc.1/src/relm4/factory/widgets/gtk.rs.html#5
impl FactoryView for SliderBox {
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
