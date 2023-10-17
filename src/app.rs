use std::sync::Arc;
use std::time::Duration;

use relm4::{gtk, ComponentParts, ComponentSender, Component, RelmWidgetExt, AsyncFactorySender};
use relm4::factory::{AsyncFactoryComponent, AsyncFactoryVecDeque};
use relm4::prelude::DynamicIndex;

use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt};
use gtk::pango::EllipsizeMode;
use gtk::{Orientation, Align, Justification};

use crate::anchor::Anchor;
use crate::colors;
use crate::server::{AudioServerEnum, AudioServer, self, Volume, Client};

pub struct App {
    server: Arc<AudioServerEnum>,
    sliders: AsyncFactoryVecDeque<Slider>,
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
    volume_percent: u8,
    muted: bool,
    name: String,
    description: String,
    #[no_eq]
    peak: f64,
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
    ServerChange(Box<Client>),
    ServerPeak(f32),
}

#[relm4::factory(async)]
impl AsyncFactoryComponent for Slider {
    type Init = server::Client;
    type Input = SliderMessage;
    type Output = Message;
    type ParentInput = Message;
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
                    set_show_fill_level: true,
                    set_restrict_to_fill_level: false,
                    #[track = "self.changed(Slider::peak())"]
                    set_fill_level: self.peak,
                    set_width_request: 1,
                    connect_fill_level_notify => |scale| {
                        let trough = scale.first_child().expect("getting GtkRange from GtkScale");
                        let fill = trough.first_child().expect("getting fill from GtkRange");

                        fill.queue_resize();
                        fill.queue_draw();
                    },
                    connect_value_changed[sender] => move |scale| {
                        sender.input(SliderMessage::ValueChange(scale.value()));
                    },
                },

                gtk::Label {
                    #[track = "self.changed(Slider::volume_percent())"]
                    set_label: &{ let mut s = self.volume_percent.to_string(); s.push('%'); s },
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

    fn forward_to_parent(msg: Self::Output) -> Option<Self::ParentInput> {
        Some(msg)
    }

    async fn init_model(init: Self::Init, _: &DynamicIndex, sender: AsyncFactorySender<Self>) -> Self {
        sender.command(|sender, shutdown| {
            shutdown
                .register(async move {
                    let mut interval = tokio::time::interval(Duration::from_millis(10));

                    loop {
                        interval.tick().await;
                        sender.emit(());
                    }
                })
                .drop_on_shutdown()
        });

        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            volume: init.volume,
            volume_percent: (init.volume.get_linear() * 100.0) as u8,
            muted: init.muted,
            peak: 0.0,

            tracker: 0,
        }
    }

    async fn update_cmd(&mut self, _: Self::CommandOutput, _: AsyncFactorySender<Self>) {
        if self.peak > 0.0 {
            self.set_peak((self.peak - 0.01).max(0.0));
        }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncFactorySender<Self>) {
       self.reset();

       match message {
           SliderMessage::Mute => {
               sender.output(Message::SetMute { id: self.id, flag: !self.muted })
           },
           SliderMessage::ValueChange(v) => {
               self.volume.set_linear(v);
               self.set_volume_percent((v * 100.0) as u8);

               sender.output(Message::VolumeChanged { id: self.id, volume: self.volume })
           },
           SliderMessage::ServerChange(client) => {
               self.set_volume(client.volume);
               self.set_volume_percent((client.volume.get_linear() * 100.0) as u8);
               self.set_muted(client.muted);
               self.set_name(client.name);
               self.set_description(client.description);
           },
           SliderMessage::ServerPeak(peak) => {
               let peak = (peak * 0.9) as f64;

               if peak > self.peak + 0.035 {
                   self.set_peak(peak + 0.015);
               }
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
            set_title:     Some(crate::APP_NAME),
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

        let sliders = AsyncFactoryVecDeque::new(gtk::Box::default(), sender.input_sender());

        let model = App { server, sliders };

        let slider_box = model.sliders.widget();
        slider_box.set_spacing(config.spacing.map(i32::from).unwrap_or(20));

        let widgets = view_output!();

        #[cfg(feature = "Wayland")]
        Self::init_wayland(window, config.anchors, &config.margins);

        #[cfg(feature = "X11")]
        window.connect_realize(move |w| Self::realize_x11(w, config.anchors, config.margins.clone()));

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        ComponentParts { model, widgets }
    }

    #[allow(unused_variables)]
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
                if let Some(index) = self.sliders.iter().flatten().position(|slider| slider.id == client.id) {
                    self.sliders.send(index, SliderMessage::ServerChange(client))
                }
            }
            Removed(id) => {
                let mut sliders = self.sliders.guard();

                let pos = sliders.iter().flatten().position(|e| e.id == id);
                if let Some(pos) = pos {
                    sliders.remove(pos);
                }
            }
            Peak(id, peak) => {
                if let Some(index) = self.sliders.iter().flatten().position(|slider| slider.id == id) {
                    self.sliders.send(index, SliderMessage::ServerPeak(peak))
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
