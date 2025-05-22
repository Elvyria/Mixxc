use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use relm4::component::{AsyncComponent, AsyncComponentSender, AsyncComponentParts};
use relm4::once_cell::sync::OnceCell;

use gtk::glib::ControlFlow;
use gtk::prelude::{ApplicationExt, GtkWindowExt, BoxExt, OrientableExt, WidgetExt, WidgetExtManual};
use gtk::Orientation;

use smallvec::SmallVec;
use tokio_util::sync::CancellationToken;

use crate::anchor::Anchor;
use crate::style::{self, StyleSettings};
use crate::widgets::sliderbox::{SliderBox, SliderMessage, Sliders};
use crate::server::{self, AudioServer, AudioServerEnum, Kind, MessageClient, MessageOutput, VolumeLevels};
use crate::widgets::switchbox::{SwitchBox, Switches};

pub static WM_CONFIG: OnceCell<WMConfig> = const { OnceCell::new() };

pub struct App {
    server: Arc<AudioServerEnum>,

    max_volume: f64,
    master: bool,
    sliders: Sliders,
    switches: Switches,
    close_after: u32,

    ready: Rc<Cell<bool>>,
    shutdown: Option<CancellationToken>,
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
    pub userstyle: Option<std::path::PathBuf>,

    #[cfg(feature = "Accent")]
    pub accent: bool,

    pub server: AudioServerEnum,
}

pub struct WMConfig {
    pub anchors: Anchor,
    pub margins: Vec<i32>,
    pub close_after: u32,
}

#[derive(Debug)]
pub enum ElementMessage {
    SetMute { ids: SmallVec<[u32; 3]>, kind: server::Kind, flag: bool },
    SetVolume { ids: SmallVec<[u32; 3]>, kind: server::Kind, levels: VolumeLevels },
    SetOutput { name: Arc<str>, port: Arc<str> },
    Remove { id: u32 },
    InterruptClose,
    Close
}

#[derive(Debug, derive_more::From)]
pub enum CommandMessage {
    #[from]
    Server(server::Message),
    SetStyle(Cow<'static, str>),
    Success,
    #[allow(dead_code)] Connect,
    Show,
    Quit,
}

#[relm4::component(pub, async)]
impl AsyncComponent for App {
    type Init = Config;
    type Input = ElementMessage;
    type Output = ();
    type CommandOutput = CommandMessage;

    view! {
        gtk::Window {
            set_resizable: false,
            set_title:     Some(crate::APP_NAME),
            set_decorated: false,

            #[name(wrapper)]
            gtk::Box {
                set_orientation: if config.horizontal {
                    Orientation::Horizontal
                } else {
                    Orientation::Vertical
                },

                #[local_ref]
                switch_box -> SwitchBox {
                    add_css_class: "side",
                    set_homogeneous: true,
                    set_orientation: if config.horizontal {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    }
                },

                #[local_ref]
                slider_box -> SliderBox {
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
    }

    fn init_loading_widgets(window: Self::Root) -> Option<relm4::loading_widgets::LoadingWidgets> {
        let config = WM_CONFIG.get().unwrap();

        #[cfg(feature = "Wayland")]
        if crate::xdg::is_wayland() {
            window.connect_realize(move |w| Self::init_wayland(w, config.anchors, &config.margins, config.close_after != 0));
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

        sender.oneshot_command(async move {
            #[allow(unused_mut)]
            let mut settings = StyleSettings::default();

            #[cfg(feature = "Accent")]
            { settings.accent = config.accent; }

            let style = match config.userstyle {
                Some(p) => style::read(p).await,
                None    => {
                    let config_dir = crate::config_dir().await.unwrap();
                    style::find(config_dir, settings).await
                },
            };

            let style = match style {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}", e);
                    style::default(settings).await
                }
            };

            CommandMessage::SetStyle(style)
        });

        App::connect(server.clone(), &sender);

        sender.oneshot_command(async move {
            use tokio::signal::*;

            let mut stream = unix::signal(unix::SignalKind::interrupt()).unwrap();
            stream.recv().await;

            CommandMessage::Quit
        });

        let mut sliders = Sliders::new(sender.input_sender());
        sliders.set_direction(wm_config.anchors, if config.horizontal { Orientation::Horizontal } else { Orientation::Vertical });
        sliders.per_process = config.per_process;

        let model = App {
            server,
            max_volume: config.max_volume,
            master: config.master,
            sliders,
            switches: Switches::new(sender.input_sender()),
            ready: Rc::new(Cell::new(false)),
            shutdown: None,
            close_after: wm_config.close_after,
        };

        let switch_box = model.switches.container.widget();
        let slider_box = model.sliders.container.widget();

        let widgets = view_output!();

        if !config.horizontal && wm_config.anchors.contains(Anchor::Bottom) {
            switch_box.insert_after(&widgets.wrapper, Some(slider_box));
        }

        window.set_default_height(config.height as i32);
        window.set_default_width(config.width as i32);

        if wm_config.close_after != 0 {
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
                    sender.input(ElementMessage::InterruptClose);
                }
                else if has_pointer.replace(false) {
                    sender.input(ElementMessage::Close);
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
            CommandMessage::Show
        });

        AsyncComponentParts { model, widgets }
    }

    async fn update_cmd(&mut self, message: Self::CommandOutput, sender: AsyncComponentSender<Self>, window: &Self::Root) {
        match message {
            CommandMessage::Server(msg) => self.handle_msg_cmd_server(msg, sender, window),
            CommandMessage::SetStyle(style) => relm4::set_global_css(&style),
            CommandMessage::Show => window.set_visible(true),
            CommandMessage::Success => {},
            CommandMessage::Connect => App::connect(self.server.clone(), &sender),
            CommandMessage::Quit => {
                self.server.disconnect();
                relm4::main_application().quit();
            },
        }
    }

    async fn update(&mut self, message: Self::Input, sender: AsyncComponentSender<Self>, _: &Self::Root) {
        use ElementMessage::*;

        match message {
            SetVolume { ids, kind, levels } => {
                self.server.set_volume(ids, kind, levels).await;
            },
            Remove { id } => {
                self.sliders.remove(id);
            }
            SetMute { ids, kind, flag } => {
                self.server.set_mute(ids, kind, flag).await;
            }
            SetOutput { name, port } => {
                self.server.set_output_by_name(&name, Some(&port)).await;
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

                let duration = Duration::from_millis(self.close_after as u64);

                sender.oneshot_command(async move {
                    tokio::select! {
                        _ = token.cancelled() => CommandMessage::Success,
                        _ = tokio::time::sleep(duration) => {
                            CommandMessage::Quit
                        }
                    }
                })
            }
        }
    }
}

impl App where App: AsyncComponent {
    fn connect(server: Arc<AudioServerEnum>, sender: &AsyncComponentSender<Self>) {
        sender.spawn_command(move |sender| match server.connect(&sender) {
            Ok(_) | Err(server::error::Error::AlreadyConnected) => {},
            Err(e) => panic!("{e}"),
        })
    }

    fn handle_msg_cmd_server(&mut self, message: server::Message, sender: AsyncComponentSender<Self>, window: &<App as AsyncComponent>::Root) {
        use server::Message::*;

        match message {
            OutputClient(msg) => self.handle_msg_output_client(msg, sender, window),
            Output(msg) => self.handle_msg_output(msg),
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

                            server.request_outputs(&sender).await.unwrap();
                            server.request_master(&sender).await.unwrap();
                        }

                        server.request_software(&sender).await.unwrap();
                        server.subscribe(plan, &sender).await.unwrap();

                        CommandMessage::Success
                    }
                });
            }
            Error(e) => eprintln!("{e}"),
            Disconnected(Some(e)) => {
                eprintln!("{e}");

                self.server.disconnect();

                self.ready.replace(false);

                self.sliders.clear();
                self.switches.clear();
            }
            Disconnected(None) => sender.command_sender().emit(CommandMessage::Quit),
        }
    }

    #[allow(unused_variables)]
    fn handle_msg_output_client(&mut self, message: MessageClient, sender: AsyncComponentSender<Self>, window: &<Self as AsyncComponent>::Root) {
        match message {
            MessageClient::Peak(id, peak) => {
                self.sliders.send(id, SliderMessage::ServerPeak(peak));
            },
            MessageClient::Changed(client) => {
                self.sliders.send(client.id, SliderMessage::ServerChange(client));
            },
            MessageClient::New(client) => {
                let mut client = *client;
                client.max_volume = f64::min(client.max_volume, self.max_volume);

                self.sliders.push_client(client);

                #[cfg(feature = "X11")]
                if crate::xdg::is_x11() {
                    window.size_allocate(&window.allocation(), -1);
                }
            },
            MessageClient::Removed(id) => {
                if !self.sliders.contains(id) { return }

                self.sliders.send(id, SliderMessage::Removed);

                sender.command({
                    let sender = sender.input_sender().clone();

                    move |_, shutdown| {
                        shutdown.register(async move {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            sender.emit(ElementMessage::Remove { id })
                        })
                        .drop_on_shutdown()
                    }
                });
            },
        }
    }

    fn handle_msg_output(&mut self, msg: MessageOutput) {
        match msg {
            MessageOutput::New(output) => {
                self.switches.push(output);
            },
            MessageOutput::Master(output) => {
                self.switches.set_active(output);
            }
        }
    }
}
