use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use relm4::{gtk, RelmWidgetExt, FactorySender};
use relm4::component::{AsyncComponent, AsyncComponentSender, AsyncComponentParts};
use relm4::factory::FactoryVecDeque;
use relm4::once_cell::sync::OnceCell;
use relm4::prelude::{DynamicIndex, FactoryComponent};

use gtk::glib::{object::Cast, ControlFlow};
use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt, WidgetExtManual};
use gtk::pango::EllipsizeMode;
use gtk::{Orientation, Align, Justification};

use smallvec::SmallVec;
use tokio_util::sync::CancellationToken;

use crate::anchor::Anchor;
use crate::widgets;
use crate::server::{self, AudioServer, AudioServerEnum, Client, Kind, Volume};

pub static WM_CONFIG: OnceCell<WMConfig> = const { OnceCell::new() };

pub struct App {
    server: Arc<AudioServerEnum>,

    max_volume: f64,
    master: bool,
    sliders: Sliders,

    ready: Rc<Cell<bool>>,
    shutdown: Option<CancellationToken>,
}

struct Sliders {
    container: FactoryVecDeque<Slider>,
    direction: GrowthDirection,
    per_process: bool,
}

enum GrowthDirection {
    TopLeft,
    BottomRight,
}

impl Sliders {
    fn push_client(&mut self, client: Client) {
        let mut sliders = self.container.guard();

        if self.per_process && client.process.is_some() {
            let pos = sliders.iter_mut().position(|slider| slider.process == client.process);

            if let Some(i) = pos {
                sliders.get_mut(i).unwrap().clients.push(SmallClient::from(&client));
                sliders.drop();

                self.container.send(i, SliderMessage::Refresh);

                return
            }
        }

        match self.direction {
            GrowthDirection::TopLeft => sliders.push_front(client),
            GrowthDirection::BottomRight => sliders.push_back(client),
        };

        sliders.drop();
    }

    fn remove(&mut self, id: u32) {
        let mut sliders = self.container.guard();

        let i = sliders.iter_mut().position(|slider| {
            match slider.clients.iter().position(|client| client.id == id) {
                Some(pos) => {
                    slider.clients.remove(pos);
                    true
                }
                None => false,
            }
        });

        match i {
            Some(i) if sliders.get(i).unwrap().clients.is_empty() => {
                sliders.remove(i);
                sliders.drop();
            }
            Some(i) => {
                sliders.drop();
                self.container.send(i, SliderMessage::Refresh);
            }
            _ => {}
        }
    }

    fn clear(&mut self) {
        self.container.guard().clear();
    }

    fn contains(&self, id: u32) -> bool {
        self.container
            .iter()
            .any(|e| e.clients.iter().any(|c| c.id == id))
    }

    fn position(&self, id: u32) -> Option<usize> {
        self.container.iter().position(|slider| slider.clients.iter().any(|c| c.id == id))
    }

    fn send(&self, id: u32, message: SliderMessage) {
        if let Some(index) = self.position(id) {
            self.container.send(index, message)
        }
    }
}

pub struct Config {
    pub width:   u32,
    pub height:  u32,
    pub spacing: i32,
    pub max_volume: f64,
    pub show_icons: bool,
    pub horizontal: bool,
    pub master: bool,
    pub show_corked: bool,
    pub per_process: bool,

    pub server: AudioServerEnum,
}

pub struct WMConfig {
    pub anchors: Anchor,
    pub margins: Vec<i32>,
    pub keep:    bool,
}

#[tracker::track]
struct Slider {
    #[do_not_track] clients: SmallVec<[SmallClient; 3]>,
    #[do_not_track] process: Option<u32>,
    volume: Volume,
    volume_percent: u8,
    muted: bool,
    corked: bool,
    name: String,
    icon: Cow<'static, str>,
    #[no_eq] peak: f64,
    removed: bool,
    updated: bool,
    #[do_not_track] kind: server::Kind,
    #[do_not_track] corking: bool,
}

impl Slider {
    fn is_corked(&self) -> bool {
        self.clients.iter().all(|id| id.corked)
    }

    fn is_muted(&self) -> bool {
        self.clients.iter().all(|id| id.muted)
    }

    fn description(&self) -> &str {
        self.clients.iter().reduce(|acc, c| {
            let a = acc.score();
            let b = c.score();

            match (a > b) || (a == b && c.id > acc.id) {
                true => c,
                false => acc,
            }
        })
        .map(|client| client.description.as_str())
        .unwrap_or_default()
    }
}

#[derive(Default, Clone)]
struct SmallClient {
    id: u32,
    description: String,
    corked: bool,
    muted: bool,
}

impl SmallClient {
    fn score(&self) -> u8 {
        (!self.corked as u8) << 3 |
        (!self.muted  as u8) << 2 |
        (!self.description.is_empty() as u8)
    }
}

impl PartialEq for SmallClient {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl From<&Client> for SmallClient {
    fn from(c: &Client) -> Self {
        Self {
            id:          c.id,
            description: c.description.clone(),
            corked:      c.corked,
            muted:       c.muted,
        }
    }
}

#[derive(Debug)]
pub enum Message {
    SetMute { ids: SmallVec<[u32; 3]>, kind: server::Kind, flag: bool },
    VolumeChanged { ids: SmallVec<[u32; 3]>, kind: server::Kind, volume: Volume, },
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
    Refresh,
}

#[derive(Debug)]
pub enum SliderCommand {
    Peak,
    Cork,
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

            #[track = "self.changed(Slider::corked())"]
            set_visible: {
                let parent = root.parent().expect("Slider has a parent")
                    .downcast::<widgets::SliderBox>().expect("Slider parent is a SliderBox");

                !self.corked || parent.show_corked()
            },

            #[track = "self.changed(Slider::removed())"]
            set_class_active: ("new", !self.removed),

            #[track = "self.changed(Slider::removed())"]
            set_class_active: ("removed", self.removed),

            #[track = "self.changed(Slider::muted())"]
            set_class_active: ("muted", self.is_muted()),

            gtk::Image {
                add_css_class: "icon",
                set_use_fallback: false,
                #[track = "self.changed(Slider::icon())"]
                set_icon_name: Some(&self.icon),
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
                    #[track = "self.changed(Slider::updated())"]
                    set_label: self.description(),
                    add_css_class: "description",
                    set_ellipsize: EllipsizeMode::End,
                },

                #[name(scale_wrapper)]
                gtk::Box {
                    #[name(scale)] // 0.00004 is a rounding error
                    gtk::Scale::with_range(Orientation::Horizontal, 0.0, parent.max_value() + 0.00004, 0.005) {
                        #[track = "self.changed(Slider::volume())"]
                        set_value: self.volume.percent(),
                        set_slider_size_fixed: true,
                        set_show_fill_level: true,
                        set_restrict_to_fill_level: false,
                        #[track = "self.changed(Slider::peak())"]
                        set_fill_level: self.peak,
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
            _ => unreachable!("Slider recieved an unknown orientation from parent"),
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

        let volume_percent = (init.volume.percent() * 100.0) as u8;
        let clients = [
            SmallClient::from(&init),
            SmallClient::default(),
            SmallClient::default()
        ];

        Self {
            clients: SmallVec::from_buf_and_len(clients, 1),
            process: init.process,
            name: init.name,
            icon: client_icon(init.icon, volume_percent, init.muted),
            volume: init.volume,
            volume_percent,
            muted: init.muted,
            corked: init.corked,
            peak: 0.0,
            removed: false,
            kind: init.kind,
            updated: false,

            corking: false,

            tracker: 0,
        }
    }

    fn update_cmd(&mut self, cmd: Self::CommandOutput, _: FactorySender<Self>) {
        self.reset();

        match cmd {
            SliderCommand::Peak => if self.peak > 0.0 {
                self.set_peak((self.peak - 0.01).max(0.0));
            },
            SliderCommand::Cork => if self.corking {
                self.corking = false;
                self.set_corked(!self.corked);
            }
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
                   self.set_peak(100.0 * self.peak * v / self.volume_percent as f64);
               }

               self.volume.set_percent(v);

               let _ = sender.output(Message::VolumeChanged {
                   ids: self.clients.iter().map(|client| client.id).collect(),
                   kind: self.kind,
                   volume: self.volume.clone()
               });
           },
           SliderMessage::Mute => {
               let _ = sender.output(Message::SetMute {
                   ids: self.clients.iter().map(|client| client.id).collect(),
                   kind: self.kind,
                   flag: !self.is_muted()
               });
           },
           SliderMessage::Removed => {
               self.set_removed(true);
           }
           SliderMessage::ServerChange(client) => {
               if let Some(existing) = self.clients.iter_mut().find(|c| c.id == client.id) {
                   *existing = client.as_ref().into();
               }

               self.set_volume_percent((client.volume.percent() * 100.0) as u8);
               self.set_volume(client.volume);
               self.set_name(client.name);
               self.set_muted(self.is_muted());
               self.set_icon(client_icon(client.icon, self.volume_percent, self.muted));
               self.set_updated(true);

               if !self.corking && (self.corked != self.is_corked()) {
                   sender.oneshot_command(async move {
                       tokio::time::sleep(Duration::from_millis(45)).await;
                       SliderCommand::Cork
                   })
               }

               self.corking = self.corked != self.is_corked();
           },
           SliderMessage::Refresh => {
               self.set_muted(self.is_muted());
               self.set_corked(self.is_corked());
               self.set_updated(true);
           }
       }
    }
}

#[relm4::component(pub, async)]
impl AsyncComponent for App {
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
                set_show_corked: config.show_corked,
                set_spacing:     config.spacing,
                set_max_value:   config.max_volume,
                set_orientation: if config.horizontal {
                    Orientation::Horizontal
                } else {
                    Orientation::Vertical
                }
            }
        }
    }

    fn init_loading_widgets(window: Self::Root) -> Option<relm4::loading_widgets::LoadingWidgets> {
        let config = WM_CONFIG.get().unwrap();

        #[cfg(feature = "Wayland")]
        if crate::xdg::is_wayland() {
            window.connect_realize(move |w| Self::init_wayland(w, config.anchors, &config.margins, !config.keep));
        }

        #[cfg(feature = "X11")]
        if crate::xdg::is_x11() {
            window.connect_realize(move |w| Self::realize_x11(w, config.anchors, config.margins.clone()));
        }

        None
    }

    async fn init(config: Self::Init, window: Self::Root, sender: AsyncComponentSender<Self>) -> AsyncComponentParts<Self> {
        if std::env::var("GTK_DEBUG").is_err() {
            glib::log_set_writer_func(|_, _| glib::LogWriterOutput::Handled);
        }

        let wm_config = WM_CONFIG.get().unwrap();
        let server = Arc::new(config.server);

        sender.spawn_command({
            let server = server.clone();

            move |sender| {
                match server.connect(sender.clone()){
                    Ok(()) => sender.emit(server::Message::Disconnected(None)),
                    Err(e) => panic!("{e}"),
                }
            }
        });

        let sliders = FactoryVecDeque::builder()
            .launch(widgets::SliderBox::default())
            .forward(sender.input_sender(), std::convert::identity);

        let direction = match config.horizontal {
            true  if wm_config.anchors.contains(Anchor::Right) => GrowthDirection::TopLeft,
            false if wm_config.anchors.contains(Anchor::Bottom) => GrowthDirection::TopLeft,
            _ => GrowthDirection::BottomRight,
        };

        let model = App {
            server,
            max_volume: config.max_volume,
            master: config.master,
            sliders: Sliders {
                container: sliders,
                direction,
                per_process: config.per_process,
            },
            ready: Rc::new(Cell::new(false)),
            shutdown: None,
        };

        let slider_box = model.sliders.container.widget();

        let widgets = view_output!();

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        if !wm_config.keep {
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

        sender.oneshot_command(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            server::Message::Timeout
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update_cmd(&mut self, message: Self::CommandOutput, sender: AsyncComponentSender<Self>, window: &Self::Root) {
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
                if !self.sliders.contains(id) { return }

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

                let mut plan = Kind::Software
                        .union(Kind::Out);

                sender.oneshot_command({
                    let sender = sender.command_sender().clone();
                    let server = self.server.clone();
                    let master = self.master;

                    async move {
                        if master {
                            plan |= Kind::Hardware;
                            server.request_master(sender.clone()).await.unwrap();
                        }

                        server.request_software(sender.clone()).await.unwrap();
                        server.subscribe(plan, sender).await.unwrap();

                        server::Message::Timeout
                    }
                });
            }
            Timeout => window.set_visible(true),
            Error(e) => eprintln!("{e}"),
            Disconnected(Some(e)) => {
                eprintln!("{e}");

                self.server.disconnect();
                self.ready.replace(false);
                self.sliders.clear();
            }
            Disconnected(None) => relm4::main_application().quit(),
            Quit => self.server.disconnect(),
        }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>, _: &Self::Root) {
        use Message::*;

        match message {
            VolumeChanged { ids, kind, volume } => {
                self.server.set_volume(ids, kind, volume).await;
            },
            Remove { id } => {
                self.sliders.remove(id);
            }
            SetMute { ids, kind, flag } => {
                self.server.set_mute(ids, kind, flag).await;
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
