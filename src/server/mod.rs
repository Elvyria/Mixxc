use anyhow::Error;
use enum_dispatch::enum_dispatch;
use libpulse_binding::volume::{ChannelVolumes, VolumeLinear};
use relm4::Sender;

use self::pulse::Pulse;
use self::pipewire::Pipewire;

pub mod pulse;
pub mod pipewire;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Volume {
    Pulse(ChannelVolumes),

    #[allow(dead_code)]
    Pipewire,
}

impl Volume {
    pub fn set(&mut self, v: f64) {
        match self {
            Volume::Pulse(cv) => {
                cv.set(cv.len(), VolumeLinear(v).into());
            },
            Volume::Pipewire => {
                unimplemented!()
            }
        }
    }

    pub fn get(&self) -> f64 {
        match self {
            Volume::Pulse(cv) => {
                VolumeLinear::from(cv.max()).0
            }
            Volume::Pipewire => {
                unimplemented!()
            }
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
    pub muted: bool,
}

#[derive(Debug)]
pub enum Message {
    New(Client),
    Changed(Client),
    Removed(u32),
    Error(Error),
    Disconnected(Option<Error>),
}

#[enum_dispatch]
#[derive(Clone)]
pub enum AudioServerEnum {
    Pulse,
    Pipewire,
}

#[enum_dispatch(AudioServerEnum)]
pub trait AudioServer {
    fn connect(&self, sender: Sender<Message>);
    fn set_volume(&self, id: u32, volume: Volume);
}
