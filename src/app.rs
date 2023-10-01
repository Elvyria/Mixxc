use std::sync::Arc;

use relm4::{gtk, ComponentParts, ComponentSender, Component, FactorySender, RelmWidgetExt};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::FactoryComponent;

use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt};
use gtk::pango::EllipsizeMode;
use gtk::{Orientation, Align, Justification};

#[cfg(feature = "Wayland")]
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::anchor::Anchor;
use crate::colors;
use crate::server::{AudioServerEnum, AudioServer, self, Volume, Client};

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
    muted: bool,
    name: String,
    description: String,
}

#[derive(Debug)]
pub enum Message {
    SetMute { id: u32, flag: bool },
    VolumeChanged { id: u32, volume: Volume, },
    Close
}

#[derive(Debug)]
pub enum SliderMessage {
    Mute,
    ValueChange(f64),
    ServerChange(Box<Client>)
}

#[relm4::factory]
impl FactoryComponent for Slider {
    type Init = server::Client;
    type Input = SliderMessage;
    type Output = Message;
    type ParentWidget = gtk::Box;
    type CommandOutput = ();

    view! {
        root = gtk::Box {
            add_css_class: "client",
            set_orientation: Orientation::Vertical,

            #[track = "self.changed(Slider::muted())"]
            set_class_active: ("muted", self.muted),

            gtk::Label {
                #[track = "self.changed(Slider::name())"]
                set_label: &self.name,
                add_css_class: "name",
                set_halign: Align::Start,
                set_ellipsize: EllipsizeMode::End,
            },

            gtk::Label {
                #[track = "self.changed(Slider::description())"]
                set_label: &self.description,
                add_css_class: "description",
                set_halign: Align::Start,
                set_ellipsize: EllipsizeMode::End,
            },

            gtk::Box {
                set_orientation: Orientation::Horizontal,

                gtk::Scale::with_range(Orientation::Horizontal, 0.0, 1.0, 0.005) {
                    #[track = "self.changed(Slider::volume())"]
                    set_value: self.volume.get_linear(),
                    set_hexpand: true,
                    set_slider_size_fixed: false,
                    connect_value_changed[sender] => move |scale| {
                        sender.input(SliderMessage::ValueChange(scale.value()));
                    },
                },

                gtk::Label {
                    #[watch]
                    set_label: &format!("{:.0}%", self.volume.get_linear() * 100.0),
                    add_css_class: "volume",
                    set_width_chars: 5,
                    set_max_width_chars: 5,
                    set_justify: Justification::Center,
                    add_controller = gtk::GestureClick {
                        set_button: gtk::gdk::BUTTON_PRIMARY,
                        connect_released[sender] => move |_, _, _, _| {
                            sender.input(SliderMessage::Mute);
                        }
                    }
                }
            }
        }
    }

    fn init_model(init: Self::Init, _: &Self::Index, _: FactorySender<Self>) -> Self {
        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            volume: init.volume,
            muted: init.muted,

            tracker: 0,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            SliderMessage::Mute => {
                sender.output(Message::SetMute { id: self.id, flag: !self.muted })
            },
            SliderMessage::ValueChange(v) => {
                self.volume.set_linear(v);
                sender.output(Message::VolumeChanged { id: self.id, volume: self.volume })
            },
            SliderMessage::ServerChange(client) => {
                self.set_volume(client.volume);
                self.set_muted(client.muted);
                self.set_name(client.name);
                self.set_description(client.description);
            },
        }
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
            set_decorated: false,

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

        #[cfg(feature = "X11")]
        window.connect_realize(move |w| Self::realize_x11(w, config.anchors, config.margins.clone()));

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        ComponentParts { model, widgets }
    }

    fn update_cmd(&mut self, message: Self::CommandOutput, sender: ComponentSender<Self>, window: &Self::Root) {
        use server::Message::*;

        match message {
            New(client) => {
                let mut sliders = self.sliders.guard();
                sliders.push_back(*client);
                sliders.drop();

                #[cfg(feature = "X11")]
                window.size_allocate(&window.allocation(), -1);
            }
            Changed(client) => {
                if let Some(index) = self.sliders.iter().position(|slider| slider.id == client.id) {
                    self.sliders.send(index, SliderMessage::ServerChange(client))
                }
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
            SetMute { id, flag } => {
                self.server.set_mute(id, flag);
            }
            VolumeChanged { id, volume } => {
                self.server.set_volume(id, volume);
            },
            Close => {
                self.server.disconnect();
                relm4::main_application().quit();
            }
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

