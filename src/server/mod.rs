#[cfg(feature = "PipeWire")]
pub mod pipewire;
pub mod pulse;

use anyhow::Error;
use enum_dispatch::enum_dispatch;
use relm4::Sender;

#[cfg(feature = "PipeWire")]
use self::pipewire::Pipewire;
use self::pulse::Pulse;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
    Pulse(libpulse_binding::volume::ChannelVolumes),

    #[cfg(feature = "PipeWire")]
    Pipewire,
}

impl Volume {
    pub fn set_linear(&mut self, v: f64) {
        match self {
            Volume::Pulse(cv) => {
                cv.set(cv.len(), libpulse_binding::volume::VolumeLinear(v).into())
            },
            #[cfg(feature = "PipeWire")]
            Volume::Pipewire  => unimplemented!(),
        };
    }

    pub fn get_linear(&self) -> f64 {
        match self {
            Volume::Pulse(cv) => libpulse_binding::volume::VolumeLinear::from(cv.max()).0,
            #[cfg(feature = "PipeWire")]
            Volume::Pipewire  => unimplemented!(),
        }
    }
}

#[derive(Debug)]
pub struct Client {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub icon: String,
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
