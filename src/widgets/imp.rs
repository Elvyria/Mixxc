use std::cell::Cell;

use relm4::gtk;
use gtk::glib;

use glib::Properties;
use glib::subclass::types::ObjectSubclass;
use glib::subclass::object::ObjectImpl;

use gtk::prelude::ObjectExt;
use gtk::subclass::box_::BoxImpl;
use gtk::subclass::widget::WidgetImpl;
use gtk::subclass::orientable::OrientableImpl;
use gtk::subclass::prelude::DerivedObjectProperties;

#[derive(Properties, Default)]
#[properties(wrapper_type = super::SliderBox)]
pub struct SliderBox {
    #[property(get, set)]
    has_icons: Cell<bool>,
}

impl WidgetImpl for SliderBox {}
impl OrientableImpl for SliderBox {}
impl BoxImpl for SliderBox {}

#[glib::object_subclass]
impl ObjectSubclass for SliderBox {
    const NAME: &'static str = "SliderBox";
    type Type = super::SliderBox;
    type ParentType = gtk::Box;
}

#[glib::derived_properties]
impl ObjectImpl for SliderBox {}
