use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use relm4::gtk;
use relm4::once_cell::sync::OnceCell;
use relm4::{ComponentParts, ComponentSender, Component, RelmWidgetExt, FactorySender};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::{DynamicIndex, FactoryComponent};

use gtk::glib::{Cast, ControlFlow};
use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt, WidgetExtManual};
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

    ready: Rc<Cell<bool>>,
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
    pub horizontal: bool,

    pub server: AudioServerEnum,
}

#[tracker::track]
struct Slider {
    #[do_not_track]
    id: u32,
    volume: Volume,
    volume_percent: u8,
    muted: bool,
    #[do_not_track]
    max: f64,
    name: String,
    description: String,
    icon: Cow<'static, str>,
    #[no_eq]
    peak: f64,
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
}

fn client_icon(icon: Option<String>, volume_percent: u8, muted: bool) -> Cow<'static, str> {
    match icon {
        Some(name) => Cow::Owned(name),
        None => {
            let s = match volume_percent {
                _ if muted      => "audio-volume-muted",
                v if v <= 25    => "audio-volume-low",
                v if v <= 75    => "audio-volume-medium",
                _               => "audio-volume-high",
            };

            Cow::Borrowed(s)
        },
    }
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
            add_css_class: "new",

            #[track = "self.changed(Slider::removed())"]
            set_class_active: ("removed", self.removed),

            #[track = "self.changed(Slider::muted())"]
            set_class_active: ("muted", self.muted),

            gtk::Image {
                add_css_class: "icon",
                set_use_fallback: false,
                #[track = "self.changed(Slider::icon())"]
                set_from_icon_name: Some(&self.icon),
                set_visible: parent.has_icons(),
            },

            gtk::Box {
                set_orientation: Orientation::Vertical,

                #[name(name)]
                gtk::Label {
                    #[track = "self.changed(Slider::name())"]
                    set_label: &self.name,
                    add_css_class: "name",
                    set_ellipsize: EllipsizeMode::End,
                },

                #[name(description)]
                gtk::Label {
                    #[track = "self.changed(Slider::description())"]
                    set_label: &self.description,
                    add_css_class: "description",
                    set_ellipsize: EllipsizeMode::End,
                },

                #[name(scale_wrapper)]
                gtk::Box {
                    #[name(scale)] // 0.00004 is a rounding error
                    gtk::Scale::with_range(Orientation::Horizontal, 0.0, self.max + 0.00004, 0.005) {
                        #[track = "self.changed(Slider::volume())"]
                        set_value: self.volume.get_linear(),
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
        let parent = root.parent().expect("Slider has a parent")
            .downcast::<widgets::SliderBox>().expect("Slider parent is a SliderBox");

        let widgets = view_output!();

        match parent.orientation() {
            Orientation::Horizontal => {
                widgets.root.set_orientation(Orientation::Vertical);
                widgets.root.set_halign(Align::Center);

                let window = parent.toplevel_window().unwrap();
                widgets.root.set_width_request(window.default_width());

                widgets.name.set_halign(Align::Center);
                widgets.description.set_halign(Align::Center);

                widgets.scale_wrapper.set_orientation(Orientation::Vertical);

                widgets.scale.set_orientation(Orientation::Vertical);
                widgets.scale.set_vexpand(true);
                widgets.scale.set_inverted(true);
            }
            Orientation::Vertical => {
                widgets.name.set_halign(Align::Start);
                widgets.description.set_halign(Align::Start);

                widgets.scale.set_hexpand(true);
            }
            _ => panic!("Slider recieved an unknown orientation from parent"),
        }

        widgets.scale.connect_fill_level_notify({
            let trough = widgets.scale.first_child().expect("getting GtkRange from GtkScale");
            let fill = trough.first_child().expect("getting fill from GtkRange");

            move |_| fill.queue_resize()
        });

        widgets.root.add_tick_callback({
            const DELAY: Duration = Duration::from_millis(500);
            let before: OnceCell<Instant> = OnceCell::new();

            move |root, _| {
                if Instant::now() - *before.get_or_init(Instant::now) < DELAY {
                    return ControlFlow::Continue
                }

                root.remove_css_class("new");
                ControlFlow::Break
            }
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

        let volume_percent = (init.volume.get_linear() * 100.0) as u8;

        Self {
            id: init.id,
            name: init.name,
            description: init.description,
            icon: client_icon(init.icon, volume_percent, init.muted),
            volume: init.volume,
            volume_percent,
            muted: init.muted,
            max: init.max_volume,
            peak: 0.0,
            removed: false,

            tracker: 0,
        }
    }

    fn update_cmd(&mut self, cmd: Self::CommandOutput, _: FactorySender<Self>) {
        match cmd {
            SliderCommand::Peak => if self.peak > 0.0 {
                self.set_peak((self.peak - 0.01).max(0.0));
            },
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
               self.set_icon(client_icon(client.icon, self.volume_percent, self.muted));
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
                add_css_class:   "main",
                set_has_icons:   config.show_icons,
                set_spacing:     config.spacing.map(i32::from).unwrap_or(20),
                set_orientation: if config.horizontal {
                    Orientation::Horizontal
                } else {
                    Orientation::Vertical
                }
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

        let direction = match config.horizontal {
            true  if config.anchors.contains(Anchor::Right) => GrowthDirection::TopLeft,
            false if config.anchors.contains(Anchor::Bottom) => GrowthDirection::TopLeft,
            _ => GrowthDirection::BottomRight,
        };

        let model = App {
            server,
            max_volume: config.max_volume,
            sliders: Sliders {
                container: sliders,
                direction,
            },
            ready: Rc::new(Cell::new(false)),
            shutdown: None,
        };

        let slider_box = model.sliders.container.widget();

        let widgets = view_output!();

        #[cfg(feature = "Wayland")]
        if crate::xdg::is_wayland() {
            Self::init_wayland(&window, config.anchors, &config.margins, !config.keep);
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

        window.add_tick_callback({
            let ready = model.ready.clone();

            move |window, _| {
                if !ready.get() {
                    window.set_visible(false);
                }

                ControlFlow::Break
            }
        });

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
            Ready => if !self.ready.replace(true) {
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
