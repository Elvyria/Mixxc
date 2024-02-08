use relm4::Sender;

use super::{AudioServer, Message, Volume};

#[derive(Clone, Copy)]
pub struct Pipewire;

impl AudioServer for Pipewire {
    fn connect(&self, sender: Sender<Message>) {
        unimplemented!()
    }

    fn disconnect(&self) {
        unimplemented!()
    }

    fn set_volume(&self, id: u32, volume: Volume) {
        unimplemented!()
    }

    fn set_mute(&self, id: u32, flag: bool) {
        unimplemented!()
    }
}
