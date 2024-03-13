use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use relm4::gtk;
use relm4::{ComponentParts, ComponentSender, Component, RelmWidgetExt, FactorySender};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::{DynamicIndex, FactoryComponent};

use gtk::glib::Cast;
use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt};
use gtk::pango::EllipsizeMode;
use gtk::{Orientation, Align, Justification};

use tokio_util::sync::CancellationToken;

use crate::anchor::Anchor;
use crate::{label, widgets};
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
    pub show_icons: bool,

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
    #[do_not_track]
    icon: Cow<'static, str>,
    #[no_eq]
    peak: f64,
    old: bool,
    removed: bool,
}

#[derive(Debug)]
pub enum Message {
    SetMute { id: u32, flag: bool },
    VolumeChanged { id: u32, volume: Volume, },
    Remove { id: u32 },
    InterruptClose,
    Close
}

#[derive(Debug)]
pub enum SliderMessage {
    Mute,
    ValueChange(f64),
    Removed,
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
    type ParentWidget = widgets::SliderBox;
    type CommandOutput = SliderCommand;

    view! {
        root = gtk::Box {
            add_css_class: "client",

            #[track = "self.changed(Slider::old())"]
            set_class_active: ("new", !self.old),

            #[track = "self.changed(Slider::removed())"]
            set_class_active: ("removed", self.removed),

            #[track = "self.changed(Slider::muted())"]
            set_class_active: ("muted", self.muted),

            gtk::Box {
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
    }

    fn init_widgets(&mut self, _: &Self::Index, root: Self::Root, _: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget, sender: FactorySender<Self>) -> Self::Widgets {
        // 0.00004 is a rounding error
        let scale = gtk::Scale::with_range(Orientation::Horizontal, 0.0, self.max + 0.00004, 0.005);

        let parent = root.parent().expect("getting container Widget from Slider");

        // TODO: Replace with macro
        // https://github.com/Relm4/Relm4/issues/231
        if parent.downcast::<widgets::SliderBox>().expect("Slider parent is a SliderBox").has_icons() {
            let icon = gtk::Image::from_icon_name(&self.icon);
            icon.add_css_class("icon");
            icon.set_use_fallback(false);

            root.append(&icon);
        }

        let widgets = view_output!();

        scale.connect_fill_level_notify({
            let trough = scale.first_child().expect("getting GtkRange from GtkScale");
            let fill = trough.first_child().expect("getting fill from GtkRange");

            move |_| fill.queue_resize()
        });

        widgets
    }

    fn init_model(init: Self::Init, _: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        sender.command(|sender, shutdown| {
            shutdown.register(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(10));

                loop {
                    interval.tick().await;
                    sender.emit(SliderCommand::Peak);
                }
            })
            .drop_on_shutdown()
        });

        sender.oneshot_command(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            SliderCommand::MarkOld
        });

        let volume_percent = (init.volume.get_linear() * 100.0) as u8;

        let icon = match init.icon {
            Some(name) => Cow::Owned(name),
            None => {
                let s = match volume_percent {
                    v if v <= 75    => "audio-volume-medium",
                    v if v <= 25    => "audio-volume-low",
                    _ if init.muted => "audio-volume-muted",
                    _               => "audio-volume-high",
                };

                Cow::Borrowed(s)
            },
        };

        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            icon,
            volume: init.volume,
            volume_percent,
            muted: init.muted,
            max: init.max_volume,
            peak: 0.0,
            old: false,
            removed: false,

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
               if self.volume_percent != 0 {
                   self.set_peak(self.peak * v / self.volume.get_linear())
               }

               self.volume.set_linear(v);
               self.set_volume_percent((v * 100.0) as u8);

               let _ = sender.output(Message::VolumeChanged { id: self.id, volume: self.volume });
           },
           SliderMessage::Mute => {
               let _ = sender.output(Message::SetMute { id: self.id, flag: !self.muted });
           },
           SliderMessage::Removed => {
               self.set_removed(true);
           }
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
            slider_box -> widgets::SliderBox {
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
            .launch(widgets::SliderBox::default())
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
        slider_box.set_has_icons(config.show_icons);

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
            let has_pointer = Rc::new(Cell::new(false));

            let controller = gtk::EventControllerMotion::new();
            controller.connect_motion({
                let has_pointer = has_pointer.clone();
                move |_, _, _| has_pointer.set(true)
            });
            window.add_controller(controller);

            let sender = sender.clone();

            window.connect_is_active_notify(move |window| {
                if window.is_active() {
                    sender.input(Message::InterruptClose);
                }
                else if has_pointer.replace(false) {
                    sender.input(Message::Close);
                }
            });
        }

        window.connect_realize(|window| window.set_visible(false));

        // Wait for server to send server::Ready message or wait and send it ourselves.
        // This is not sound, but 'Timeout' message would not be useful enough.
        sender.oneshot_command(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            server::Message::Ready
        });

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
                self.sliders.send(id, SliderMessage::Removed);

                sender.command({
                    let sender = sender.input_sender().clone();

                    move |_, shutdown| {
                        shutdown.register(async move {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            sender.emit(Message::Remove { id })
                        })
                        .drop_on_shutdown()
                    }
                });
            }
            Changed(client) => {
                self.sliders.send(client.id, SliderMessage::ServerChange(client));
            }
            Ready => if !window.is_visible() {
                window.set_visible(true);
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
            Remove { id } => {
                self.sliders.remove(id);
            }
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
