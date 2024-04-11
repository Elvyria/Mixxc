#[cfg(feature = "PipeWire")]
pub mod pipewire;
pub mod pulse;

use std::fmt::Debug;

use anyhow::Error;
use enum_dispatch::enum_dispatch;
use relm4::Sender;

#[cfg(feature = "PipeWire")]
use self::pipewire::Pipewire;
use self::pulse::Pulse;

pub mod id {
    pub const MASTER: u32 = 0;
}

#[derive(Clone, Copy)]
pub struct Volume {
    inner: RawVolume,
    percent: &'static (dyn Fn(&RawVolume) -> f64 + Sync),
    set_percent: &'static (dyn Fn(&mut RawVolume, f64) + Sync),
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
    pub fn percent(&self) -> f64{
        (self.percent)(&self.inner)
    }

    pub fn set_percent(&mut self, p: f64) {
        (self.set_percent)(&mut self.inner, p)
    }
}

impl RawVolume {
    pub fn linear(&self) -> f64 {
        match self {
            RawVolume::Pulse(cv) => libpulse_binding::volume::VolumeLinear::from(cv.max()).0,
            #[cfg(feature = "PipeWire")]
            RawVolume::Pipewire  => unimplemented!(),
        }
    }

    pub fn set_linear(&mut self, v: f64) {
        match self {
            RawVolume::Pulse(cv) => {
                cv.set(cv.len(), libpulse_binding::volume::VolumeLinear(v).into())
            },
            #[cfg(feature = "PipeWire")]
            RawVolume::Pipewire  => unimplemented!(),
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RawVolume {
    Pulse(libpulse_binding::volume::ChannelVolumes),

    #[cfg(feature = "PipeWire")]
    Pipewire,
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

#[enum_dispatch(AudioServerEnum)]
pub trait AudioServer {
    fn connect(&self, sender: Sender<Message>);
    fn disconnect(&self);
    fn set_volume(&self, id: u32, volume: Volume);
    fn set_mute(&self, id: u32, flag: bool);
}
