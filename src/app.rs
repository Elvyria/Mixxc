use std::sync::Arc;

use gtk::prelude::{BoxExt, GtkWindowExt, OrientableExt, ScaleExt, RangeExt, WidgetExt};
use relm4::factory::FactoryVecDeque;
use relm4::gtk::pango::EllipsizeMode;
use relm4::gtk::prelude::ApplicationExt;
use relm4::gtk::{Orientation, PositionType, Overflow, Align, pango};
use relm4::prelude::FactoryComponent;
use relm4::{gtk, ComponentParts, ComponentSender, Component, FactorySender};

#[cfg(feature = "Wayland")]
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::anchor::Anchor;
use crate::colors;
use crate::server::{AudioServerEnum, AudioServer, self, Volume};

pub struct App {
    server: Arc<AudioServerEnum>,
    sliders: FactoryVecDeque<Slider>,
}

pub struct Config {
    pub width:   u32,
    pub height:  u32,
    pub spacing: Option<u16>,
    pub anchors: Anchor,
    pub margins: Vec<i32>,

    pub server: AudioServerEnum,
}

#[tracker::track]
struct Slider {
    #[do_not_track]
    id: u32,
    volume: Volume,
    name: String,
    description: String,
}

#[derive(Debug)]
pub enum Message {
    VolumeChanged {
        id: u32,
        volume: Volume,
    },
    Close
}

#[relm4::factory]
impl FactoryComponent for Slider {
    type Init = server::Client;
    type Input = f64;
    type Output = Message;
    type ParentWidget = gtk::Box;
    type CommandOutput = ();

    view! {
        root = gtk::Box {
            add_css_class: "client",
            set_orientation: Orientation::Vertical,

            gtk::Label {
                #[track = "self.changed(Slider::name())"]
                add_css_class: "name",
                set_label: &self.name,
                set_halign: Align::Start,
                set_ellipsize: EllipsizeMode::End,
            },

            gtk::Label {
                #[track = "self.changed(Slider::description())"]
                add_css_class: "description",
                set_label: &self.description,
                set_halign: Align::Start,
                set_ellipsize: EllipsizeMode::End,
            },

            gtk::Box {
                set_orientation: Orientation::Horizontal,

                gtk::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.005) {
                    #[track = "self.changed(Slider::volume())"]
                    set_value: self.volume.get(),
                    set_hexpand: true,
                    set_slider_size_fixed: false,
                    set_draw_value: true,
                    set_value_pos: PositionType::Right,
                    set_format_value_func: |_, value| format!("{:.0}%", value * 100.0),
                    connect_value_changed[sender] => move |scale| {
                        sender.input(scale.value());
                    },
                },
            }
        }
    }

    fn init_model(init: Self::Init, _: &Self::Index, _: FactorySender<Self>) -> Self {
        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            volume: init.volume,

            tracker: 0,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        self.volume.set(message);
        sender.output(Message::VolumeChanged { id: self.id, volume: self.volume });
    }
}

#[relm4::component(pub)]
impl Component for App {
    type Init = Config;
    type Input = Message;
    type Output = ();
    type CommandOutput = server::Message;

    view! {
        gtk::Window {
            set_resizable: false,

            add_controller = gtk::EventControllerMotion {
                connect_leave[sender] => move |motion| {
                    if motion.is_pointer() {
                        sender.input(Message::Close);
                    }
                }
            },

            #[local_ref]
            slider_box -> gtk::Box {
                add_css_class: "main",
                set_orientation: Orientation::Vertical,
            }
        }
    }

    fn init(config: Self::Init, window: &Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let server = Arc::new(config.server);

        sender.spawn_command({
            let server = server.clone();

            move |sender| server.connect(sender) 
        });

        let sliders = FactoryVecDeque::builder(gtk::Box::default())
            .launch()
            .forward(sender.input_sender(), std::convert::identity);

        let model = App { server, sliders };

        let slider_box = model.sliders.widget();
        slider_box.set_spacing(config.spacing.map(i32::from).unwrap_or(20));

        let widgets = view_output!();

        #[cfg(feature = "Wayland")]
        {
            window.init_layer_shell();
            window.set_layer(Layer::Top);
            window.set_title(Some(crate::APP_NAME));
            window.set_namespace("volume-mixer");

            for (i, anchor) in config.anchors.iter().enumerate() {
                let edge = anchor.try_into().unwrap();

                window.set_anchor(edge, true);
                window.set_margin(edge, *config.margins.get(i).unwrap_or(&0));
            }
        }

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        ComponentParts { model, widgets }
    }

    fn update_cmd(&mut self, message: Self::CommandOutput, sender: ComponentSender<Self>, _: &Self::Root) {
        use server::Message::*;

        match message {
            New(client) => {
                let mut sliders = self.sliders.guard();
                sliders.push_back(client);
            }
            Changed(client) => {
                let mut sliders = self.sliders.guard();

                let Some(slider) = sliders.iter_mut().find(|slider| slider.id == client.id) else {
                    return
                };

                slider.set_volume(client.volume);
                slider.set_name(client.name);
                slider.set_description(client.description);
            }
            Removed(id) => {
                let mut sliders = self.sliders.guard();

                let pos = sliders.iter().position(|e| e.id == id);
                if let Some(pos) = pos {
                    sliders.remove(pos);
                }
            }
            Error(e) => eprintln!("{}: Audio Server :{e}", colors::ERROR),
            Disconnected(Some(e)) => {
                eprintln!("{}: Audio Server :{e}", colors::ERROR);

                sender.spawn_command({
                    let server = self.server.clone();

                    move |sender| server.connect(sender) 
                });
            }
            Disconnected(None) => relm4::main_application().quit(),
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _: &Self::Root) {
        use Message::*;

        match message {
            VolumeChanged { id, volume } => {
                self.server.set_volume(id, volume);
            },
            Close => self.server.disconnect(),
        }
    }
}

#[cfg(feature = "Wayland")]
impl TryFrom<Anchor> for Edge {
    type Error = ();

    fn try_from(anchor: Anchor) -> Result<Self, ()> {
        match anchor {
            Anchor::Top    => Ok(Edge::Top),
            Anchor::Left   => Ok(Edge::Left),
            Anchor::Bottom => Ok(Edge::Bottom),
            Anchor::Right  => Ok(Edge::Right),
            _              => Err(())
        }
    }
}
