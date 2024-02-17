use std::sync::Arc;
use std::time::Duration;

use relm4::gtk::glib::ControlFlow;
use relm4::gtk::prelude::WidgetExtManual;
use relm4::{gtk, ComponentParts, ComponentSender, Component, RelmWidgetExt, FactorySender};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::{DynamicIndex, FactoryComponent};

use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt};
use gtk::pango::EllipsizeMode;
use gtk::{Orientation, Align, Justification};

use tokio_util::sync::CancellationToken;

use crate::anchor::Anchor;
use crate::label;
use crate::server::{AudioServerEnum, AudioServer, self, Volume, Client};

pub struct App {
    server: Arc<AudioServerEnum>,

    max_volume: f64,
    sliders: Sliders,

    shutdown: Option<CancellationToken>,
}

struct Sliders {
    container: FactoryVecDeque<Slider>,
    direction: GrowthDirection,
}

enum GrowthDirection {
    TopLeft,
    BottomRight,
}

impl Sliders {
    fn push_client(&mut self, client: Client) {
        let mut sliders = self.container.guard();

        match self.direction {
            GrowthDirection::TopLeft => sliders.push_front(client),
            GrowthDirection::BottomRight => sliders.push_back(client),
        };

        sliders.drop();
    }

    fn remove(&mut self, id: u32) {
        let mut sliders = self.container.guard();

        let pos = sliders.iter().position(|e| e.id == id);
        if let Some(pos) = pos {
            sliders.remove(pos);
        }

        sliders.drop();
    }

    fn send(&self, id: u32, message: SliderMessage) {
        if let Some(index) = self.container.iter().position(|slider| slider.id == id) {
            self.container.send(index, message)
        }
    }
}

pub struct Config {
    pub width:   u32,
    pub height:  u32,
    pub spacing: Option<u16>,
    pub anchors: Anchor,
    pub margins: Vec<i32>,
    pub keep: bool,
    pub max_volume: f64,

    pub server: AudioServerEnum,
}

#[tracker::track]
struct Slider {
    #[do_not_track]
    id: u32,
    volume: Volume,
    volume_percent: u8,
    muted: bool,
    max: f64,
    name: String,
    description: String,
    #[no_eq]
    peak: f64,
    old: bool,
}

#[derive(Debug)]
pub enum Message {
    SetMute { id: u32, flag: bool },
    VolumeChanged { id: u32, volume: Volume, },
    InterruptClose,
    Close
}

#[derive(Debug)]
pub enum SliderMessage {
    Mute,
    ValueChange(f64),
    ServerChange(Box<Client>),
    ServerPeak(f32),
}

#[derive(Debug)]
pub enum SliderCommand {
    Peak,
    MarkOld,
}

#[relm4::factory()]
impl FactoryComponent for Slider {
    type Init = server::Client;
    type Input = SliderMessage;
    type Output = Message;
    type ParentWidget = gtk::Box;
    type CommandOutput = SliderCommand;

    view! {
        root = gtk::Box {
            add_css_class: "client",

            #[track = "self.changed(Slider::old())"]
            set_class_active: ("new", !self.old),

            #[track = "self.changed(Slider::muted())"]
            set_class_active: ("muted", self.muted),

            set_orientation: Orientation::Vertical,

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

                #[local_ref]
                scale -> gtk::Scale {
                    #[track = "self.changed(Slider::volume())"]
                    set_value: self.volume.get_linear(),
                    set_hexpand: true,
                    set_slider_size_fixed: false,
                    set_show_fill_level: true,
                    set_restrict_to_fill_level: false,
                    #[track = "self.changed(Slider::peak())"]
                    set_fill_level: self.peak,
                    set_width_request: 1,
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

    fn init_widgets(&mut self, _: &Self::Index, root: Self::Root, _: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget, sender: FactorySender<Self>) -> Self::Widgets {
        // 0.00004 is a rounding error
        let scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, self.max + 0.00004, 0.005);

        let widgets = view_output!();

        {
            let sender = sender.command_sender().clone();

            widgets.scale.add_tick_callback(move |_, _| {
                sender.emit(SliderCommand::Peak);

                ControlFlow::Continue
            });
        }

        {
            let scale = &widgets.scale;

            let trough = scale.first_child().expect("getting GtkRange from GtkScale");
            let fill = trough.first_child().expect("getting fill from GtkRange");

            scale.connect_fill_level_notify(move |_| fill.queue_resize());
        }

        widgets
    }

    fn init_model(init: Self::Init, _: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        sender.oneshot_command(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            SliderCommand::MarkOld
        });

        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            volume: init.volume,
            volume_percent: (init.volume.get_linear() * 100.0) as u8,
            muted: init.muted,
            max: init.max_volume,
            peak: 0.0,
            old: false,

            tracker: 0,
        }
    }

    fn update_cmd(&mut self, cmd: Self::CommandOutput, _: FactorySender<Self>) {
        match cmd {
            SliderCommand::Peak => if self.peak > 0.0 {
                self.set_peak((self.peak - 0.01).max(0.0));
            },
            SliderCommand::MarkOld => self.set_old(true),
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
       self.reset();

       match message {
           SliderMessage::ServerPeak(peak) => {
               let peak = (peak * 0.9) as f64;

               if peak > self.peak + 0.035 {
                   self.set_peak(peak + 0.015);
               }
           },
           SliderMessage::ValueChange(v) => {
               self.set_peak(self.peak * v / self.volume.get_linear());

               self.volume.set_linear(v);
               self.set_volume_percent((v * 100.0) as u8);

               let _ = sender.output(Message::VolumeChanged { id: self.id, volume: self.volume });
           },
           SliderMessage::Mute => {
               let _ = sender.output(Message::SetMute { id: self.id, flag: !self.muted });
           },
           SliderMessage::ServerChange(client) => {
               self.set_volume(client.volume);
               self.set_volume_percent((client.volume.get_linear() * 100.0) as u8);
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
            set_title:     Some(crate::APP_NAME),
            set_decorated: false,

            #[local_ref]
            slider_box -> gtk::Box {
                add_css_class: "main",
                set_orientation: Orientation::Vertical,
            }
        }
    }

    fn init(config: Self::Init, window: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let server = Arc::new(config.server);

        sender.spawn_command({
            let server = server.clone();

            move |sender| server.connect(sender)
        });

        let sliders = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), std::convert::identity);

        let model = App {
            server,
            max_volume: config.max_volume,
            sliders: Sliders {
                container: sliders,
                direction: if config.anchors.contains(Anchor::Bottom) {
                    GrowthDirection::TopLeft
                } else {
                    GrowthDirection::BottomRight
                },
            },
            shutdown: None,
        };

        let slider_box = model.sliders.container.widget();
        slider_box.set_spacing(config.spacing.map(i32::from).unwrap_or(20));

        let widgets = view_output!();

        #[cfg(feature = "Wayland")]
        if crate::xdg::is_wayland() {
            Self::init_wayland(&window, config.anchors, &config.margins);
        }

        #[cfg(feature = "X11")]
        if crate::xdg::is_x11() {
            window.connect_realize(move |w| Self::realize_x11(w, config.anchors, config.margins.clone()));
        }

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        if !config.keep {
            let controller = gtk::EventControllerMotion::new();
            controller.connect_leave({
                let sender = sender.clone();
                move |motion| {
                    if motion.is_pointer() {
                        sender.input(Message::Close);
                    }
                }
            });
            controller.connect_enter({
                let sender = sender.clone();
                move |motion, _, _| {
                    if motion.is_pointer() {
                        sender.input(Message::InterruptClose);
                    }
                }
            });
            window.add_controller(controller);
        }

        // Wait a tiny bit for server thread to get ready.
        // This helps to skip initial empty window without introducing sync boilerplate.
        std::thread::yield_now();

        ComponentParts { model, widgets }
    }

    #[allow(unused_variables)]
    fn update_cmd(&mut self, message: Self::CommandOutput, sender: ComponentSender<Self>, window: &Self::Root) {
        use server::Message::*;

        match message {
            Peak(id, peak) => {
                self.sliders.send(id, SliderMessage::ServerPeak(peak));
            }
            New(client) => {
                let mut client = *client;
                client.max_volume = f64::min(client.max_volume, self.max_volume);

                self.sliders.push_client(client);

                #[cfg(feature = "X11")]
                if crate::xdg::is_x11() {
                    window.size_allocate(&window.allocation(), -1);
                }
            }
            Removed(id) => {
                self.sliders.remove(id)
            }
            Changed(client) => {
                self.sliders.send(client.id, SliderMessage::ServerChange(client));
            }
            Error(e) => eprintln!("{}: Audio Server :{e}", label::ERROR),
            Disconnected(Some(e)) => {
                eprintln!("{}: Audio Server :{e}", label::ERROR);

                sender.spawn_command({
                    let server = self.server.clone();

                    move |sender| server.connect(sender) 
                });
            }
            Disconnected(None) => relm4::main_application().quit(),
            Quit => {
                self.server.disconnect();
                relm4::main_application().quit();
            }
        }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _: &Self::Root) {
        use Message::*;

        match message {
            VolumeChanged { id, volume } => {
                self.server.set_volume(id, volume);
            },
            SetMute { id, flag } => {
                self.server.set_mute(id, flag);
            }
            InterruptClose => {
                if let Some(shutdown) = self.shutdown.take() {
                    shutdown.cancel();
                }
            },
            Close => {
                if let Some(shutdown) = self.shutdown.take() {
                    shutdown.cancel();
                }

                self.shutdown = Some(CancellationToken::new());
                let token = self.shutdown.as_ref().unwrap().clone();

                sender.command(|sender, shutdown| {
                    shutdown.register(async move {
                        tokio::select! {
                            _ = token.cancelled() => {}
                            _ = tokio::time::sleep(Duration::from_millis(150)) => {
                                sender.emit(server::Message::Quit);
                            }
                        }
                    })
                    .drop_on_shutdown()
                });
            }
        }
    }
}
