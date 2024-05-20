#[cfg(feature = "PipeWire")]
pub mod pipewire;
pub mod pulse;
pub mod error;

use std::fmt::Debug;

use enum_dispatch::enum_dispatch;
use relm4::Sender;

use error::Error;

#[cfg(feature = "PipeWire")]
use self::pipewire::Pipewire;
use self::pulse::Pulse;

#[derive(Clone)]
pub struct Volume {
    inner: smallvec::SmallVec<[u32; 2]>,
    percent: &'static (dyn Fn(&Self) -> f64 + Sync),
    set_percent: &'static (dyn Fn(&mut Self, f64) + Sync),
}

impl PartialEq for Volume {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Debug for Volume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
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

#[derive(Debug)]
pub struct Client {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub volume: Volume,
    pub max_volume: f64,
    pub muted: bool,
    pub corked: bool,
    pub kind: Kind,
}

#[derive(Debug)]
pub enum Message {
    New(Box<Client>),
    Changed(Box<Client>),
    Removed(u32),
    Peak(u32, f32),
    Ready,
    Error(Error),
    Disconnected(Option<Error>),
    Quit,
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
    fn connect(&self, sender: Sender<Message>) -> Result<(), Error>;
    fn disconnect(&self);
    fn request_software(&self, sender: Sender<Message>) -> Result<(), Error>;
    fn request_master(&self, sender: Sender<Message>) -> Result<(), Error>;
    fn subscribe(&self, plan: Kind, sender: Sender<Message>) -> Result<(), Error>;
    fn set_volume(&self, id: u32, kind: Kind, volume: Volume);
    fn set_mute(&self, id: u32, kind: Kind, flag: bool);
}
