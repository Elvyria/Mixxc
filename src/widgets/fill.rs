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
#[properties(wrapper_type = super::Fill)]
pub struct Fill {
    #[property(get, set)]
    value: Cell<f64>,
}

impl WidgetImpl for Fill {}
impl OrientableImpl for Fill {}
impl BoxImpl for Fill {}

#[glib::object_subclass]
impl ObjectSubclass for Fill {
    const NAME: &'static str = "CustomFill";
    type Type = super::Fill;
    type ParentType = gtk::Widget;
}

#[glib::derived_properties]
impl ObjectImpl for Fill {}
