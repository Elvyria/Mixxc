use std::borrow::Cow;
use std::time::{Duration, Instant};

use relm4::Sender;
use relm4::{RelmWidgetExt, FactorySender};
use relm4::factory::{FactoryView, FactoryVecDeque};
use relm4::once_cell::sync::OnceCell;
use relm4::prelude::{DynamicIndex, FactoryComponent};

use gtk::{Orientation, Align, Justification};
use gtk::pango::EllipsizeMode;
use gtk::glib::{self, Object, object::Cast, ControlFlow};
use gtk::prelude::{BoxExt, GtkWindowExt, GestureSingleExt, OrientableExt, RangeExt, WidgetExt, WidgetExtManual};

use smallvec::SmallVec;

use crate::anchor::Anchor;
use crate::app::Message;
use crate::server::{self, OutputClient, Volume};

use super::GrowthDirection;

#[derive(Debug)]
pub enum SliderMessage {
    Mute,
    ValueChange(f64),
    Removed,
    ServerChange(Box<OutputClient>),
    ServerPeak(f32),
    Refresh,
}

#[derive(Debug)]
pub enum SliderCommand {
    Peak,
    Cork,
}

pub struct Sliders {
    pub container: FactoryVecDeque<Slider>,
    pub direction: GrowthDirection,
    pub per_process: bool,
}

impl Sliders {
    pub fn new(sender: &Sender<Message>) -> Self {
        let container = FactoryVecDeque::builder()
            .launch(SliderBox::default())
            .forward(sender, std::convert::identity);

        Self {
            container,
            direction: GrowthDirection::BottomRight,
            per_process: false,
        }
    }

    pub fn set_direction(&mut self, anchor: Anchor, orientation: Orientation) {
        self.direction = match orientation {
            Orientation::Horizontal if anchor.contains(Anchor::Right) => GrowthDirection::TopLeft,
            Orientation::Vertical if anchor.contains(Anchor::Bottom) => GrowthDirection::TopLeft,
            _ => GrowthDirection::BottomRight,
        };
    }

    pub fn push_client(&mut self, client: OutputClient) {
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

    pub fn remove(&mut self, id: u32) {
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

    pub fn clear(&mut self) {
        self.container.guard().clear();
    }

    pub fn contains(&self, id: u32) -> bool {
        self.container
            .iter()
            .any(|e| e.clients.iter().any(|c| c.id == id))
    }

    fn position(&self, id: u32) -> Option<usize> {
        self.container.iter().position(|slider| slider.clients.iter().any(|c| c.id == id))
    }

    pub fn send(&self, id: u32, message: SliderMessage) {
        if let Some(index) = self.position(id) {
            self.container.send(index, message)
        }
    }
}

#[tracker::track]
pub struct Slider {
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
    #[no_eq] updated: bool,
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
        if self.clients.len() == 1 {
            return self.clients[0].description.as_str()
        }

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

#[relm4::factory(pub)]
impl FactoryComponent for Slider {
    type Init = server::OutputClient;
    type Input = SliderMessage;
    type Output = Message;
    type ParentWidget = SliderBox;
    type CommandOutput = SliderCommand;

    view! {
        root = gtk::Box {
            add_css_class: "client",

            #[track = "self.changed(Self::corked())"]
            set_visible: {
                let parent = root.parent().expect("Slider has a parent")
                    .downcast::<Self::ParentWidget>().expect("Slider parent is a SliderBox");

                !self.corked || parent.show_corked()
            },

            #[track = "self.changed(Self::removed())"]
            set_class_active: ("new", !self.removed),

            #[track = "self.changed(Self::removed())"]
            set_class_active: ("removed", self.removed),

            #[track = "self.changed(Self::muted())"]
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
                    #[track = "self.changed(Self::name())"]
                    set_label: &self.name,
                    add_css_class: "name",
                    set_ellipsize: EllipsizeMode::End,
                },

                #[name(description)]
                gtk::Label {
                    #[track = "self.changed(Self::updated())"]
                    set_label: self.description(),
                    add_css_class: "description",
                    set_ellipsize: EllipsizeMode::End,
                },

                #[name(scale_wrapper)]
                gtk::Box {
                    #[name(scale)] // 0.00004 is a rounding error
                    gtk::Scale::with_range(Orientation::Horizontal, 0.0, parent.max_value() + 0.00004, 0.005) {
                        #[track = "self.changed(Self::volume())"]
                        set_value: self.volume.percent(),
                        set_slider_size_fixed: true,
                        set_show_fill_level: true,
                        set_restrict_to_fill_level: false,
                        #[track = "self.changed(Self::peak())"]
                        set_fill_level: self.peak,
                        connect_value_changed[sender] => move |scale| {
                            sender.input(SliderMessage::ValueChange(scale.value()));
                        },
                    },

                    gtk::Label {
                        #[track = "self.changed(Self::volume_percent())"]
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
            .downcast::<Self::ParentWidget>().expect("Slider parent is a SliderBox");

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

               let _ = sender.output(Message::SetVolume {
                   ids: self.clients.iter().map(|client| client.id).collect(),
                   kind: self.kind,
                   levels: self.volume.levels.clone()
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
                   let new: SmallClient = client.as_ref().into();

                   // TODO: This is really wasteful, please do something about it T_T
                   if *existing != new {
                       *existing = new;
                       self.set_updated(true);
                   }
               }

               self.set_volume_percent((client.volume.percent() * 100.0) as u8);
               self.set_volume(client.volume);
               self.set_name(client.name);
               self.set_icon(client_icon(client.icon, self.volume_percent, self.muted));

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

#[derive(Default, PartialEq, Clone)]
pub struct SmallClient {
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

impl From<&OutputClient> for SmallClient {
    fn from(c: &OutputClient) -> Self {
        Self {
            id:          c.id,
            description: c.description.clone(),
            corked:      c.corked,
            muted:       c.muted,
        }
    }
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

mod imp {
    use std::cell::Cell;

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

        #[property(get, set)]
        show_corked: Cell<bool>,

        #[property(get, set)]
        max_value: Cell<f64>,
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
}

// This is a 99% GTK boilerplate to get a custom Widget.
// https://gtk-rs.org/gtk4-rs/git/book/g_object_subclassing.html
glib::wrapper! {
    pub struct SliderBox(ObjectSubclass<imp::SliderBox>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SliderBox {
    pub fn default() -> Self {
        Object::builder().build()
    }
}

// FactoryComponent::ParentWidget must implement FactoryView.
// This is the same implementation that is used for gtk::Box.
// https://docs.rs/relm4/0.7.0-rc.1/src/relm4/factory/widgets/gtk.rs.html#5
impl FactoryView for SliderBox {
    type Children = gtk::Widget;
    type ReturnedWidget = gtk::Widget;
    type Position = ();

    fn factory_remove(&self, widget: &Self::ReturnedWidget) {
        self.remove(widget);
    }

    fn factory_append(&self, widget: impl AsRef<Self::Children>, _: &Self::Position) -> Self::ReturnedWidget {
        self.append(widget.as_ref());
        widget.as_ref().clone()
    }

    fn factory_prepend(&self, widget: impl AsRef<Self::Children>, _: &Self::Position) -> Self::ReturnedWidget {
        self.prepend(widget.as_ref());
        widget.as_ref().clone()
    }

    fn factory_insert_after(&self, widget: impl AsRef<Self::Children>, _: &Self::Position, other: &Self::ReturnedWidget) -> Self::ReturnedWidget {
        self.insert_child_after(widget.as_ref(), Some(other));
        widget.as_ref().clone()
    }

    fn returned_widget_to_child(returned_widget: &Self::ReturnedWidget) -> Self::Children {
        returned_widget.clone()
    }

    fn factory_move_after(&self, widget: &Self::ReturnedWidget, other: &Self::ReturnedWidget) {
        self.reorder_child_after(widget, Some(other));
    }

    fn factory_move_start(&self, widget: &Self::ReturnedWidget) {
        self.reorder_child_after(widget, None::<&gtk::Widget>);
    }
}
