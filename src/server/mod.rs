#[cfg(feature = "PipeWire")]
pub mod pipewire;
pub mod pulse;
pub mod error;

use derive_more::derive::{Debug, Deref, DerefMut};

use enum_dispatch::enum_dispatch;

use error::Error;

#[cfg(feature = "PipeWire")]
use self::pipewire::Pipewire;
use self::pulse::Pulse;

pub struct InnerSender<T, U> {
    sender: relm4::Sender<T>,
    _message: std::marker::PhantomData<U>,
}

impl<T, U> InnerSender<T, U> {
    #[inline]
    pub fn emit(&self, message: impl Into<T>) {
        self.sender.emit(message.into())
    }

    pub fn clone(&self) -> Self {
        InnerSender {
            sender: self.sender.clone(),
            _message: self._message
        }
    }
}

impl<T, U> From<&relm4::Sender<T>> for InnerSender<T, U> {
    fn from(sender: &relm4::Sender<T>) -> Self {
        Self {
            sender: sender.clone(),
            _message: std::marker::PhantomData,
        }
    }
}

pub type Sender<T> = InnerSender<crate::app::CommandMessage, T>;

#[derive(Debug, Clone, Deref, DerefMut)]
pub struct VolumeLevels(smallvec::SmallVec<[u32; 2]>);

#[derive(Debug, Clone)]
pub struct Volume {
    pub levels: VolumeLevels,

    #[debug(skip)]
    percent: &'static (dyn Fn(&Self) -> f64 + Sync),

    #[debug(skip)]
    set_percent: &'static (dyn Fn(&mut Self, f64) + Sync),
}

impl PartialEq for Volume {
    fn eq(&self, other: &Self) -> bool {
        self.levels[0] == other.levels[0]
    }
}

impl Volume {
    pub fn percent(&self) -> f64 {
        (self.percent)(self)
    }

    pub fn set_percent(&mut self, p: f64) {
        (self.set_percent)(self, p)
    }
}

#[derive(Debug, Clone)]
pub struct OutputClient {
    pub id: u32,
    pub process: Option<u32>,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub volume: Volume,
    pub max_volume: f64,
    pub muted: bool,
    pub corked: bool,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub name: String,
    pub port: String,
    pub master: bool,
}

#[derive(Debug)]
pub enum Message {
    Output(MessageOutput),
    OutputClient(MessageClient),
    Disconnected(Option<Error>),
    Error(Error),
    Ready,
}

#[derive(Debug)]
pub enum MessageClient {
    New(Box<OutputClient>),
    Changed(Box<OutputClient>),
    Removed(u32),
    Peak(u32, f32),
}

impl From<MessageClient> for Message {
    fn from(msg: MessageClient) -> Self {
        Message::OutputClient(msg)
    }
}

#[derive(Debug)]
pub enum MessageOutput {
    New(Output),
    Master(Output),
}

impl From<MessageOutput> for Message {
    fn from(msg: MessageOutput) -> Self {
        Message::Output(msg)
    }
}

#[enum_dispatch]
pub enum AudioServerEnum {
    Pulse,
    #[cfg(feature = "PipeWire")]
    Pipewire,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct Kind: u8 {
        const Software = 0b0001;
        const Hardware = 0b0010;
        const Out      = 0b0100;
        const In       = 0b1000;
    }
}

#[enum_dispatch(AudioServerEnum)]
pub trait AudioServer {
    fn connect(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error>;
    fn disconnect(&self);
    async fn request_software(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error>;
    async fn request_master(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error>;
    async fn request_outputs(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error>;
    async fn subscribe(&self, plan: Kind, sender: impl Into<Sender<Message>>) -> Result<(), Error>;
    async fn set_volume(&self, ids: impl IntoIterator<Item = u32>, kind: Kind, levels: VolumeLevels);
    async fn set_mute(&self, ids: impl IntoIterator<Item = u32>, kind: Kind, flag: bool);
    async fn set_output_by_name(&self, name: &str, port: Option<&str>);
}
