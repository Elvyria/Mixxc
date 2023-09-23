use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use anyhow::anyhow;
use relm4::Sender;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{InterestMaskSet, Facility, Operation};
use libpulse_binding::context::introspect::SinkInputInfo;
use libpulse_binding::context::{Context, FlagSet};
use libpulse_binding::mainloop::standard::{Mainloop, IterateResult};
use libpulse_binding::proplist::Proplist;
use libpulse_binding::proplist::properties::APPLICATION_NAME;
use libpulse_binding::volume::ChannelVolumes;

use super::{Message, Volume, AudioServer, Client};

#[derive(Copy, Clone)]
enum Command {
    SetVolume(u32, ChannelVolumes),
}

// use super::AudioServer;
#[derive(Clone)]
pub struct Pulse {
    tx: flume::Sender<Command>,
    rx: flume::Receiver<Command>,
}

impl Pulse {
    pub fn new() -> Self {
        let (tx, rx) = flume::bounded(8);

        Self { tx, rx }
    }
}

impl AudioServer for Pulse {
    fn connect(&self, sender: Sender<Message>) {
        let mut proplist = Proplist::new().unwrap();
        proplist.set_str(APPLICATION_NAME, crate::APP_NAME).unwrap();

        let mut mainloop = Mainloop::new().unwrap();

        let context = Context::new_with_proplist(&mainloop, "Mixxc Context", &proplist).unwrap();
        let context = Rc::new(RefCell::new(context));

        let state_callback = Box::new({
            let context = context.clone();
            let sender = sender.clone();

            move || print_state(&sender, &context)
        });

        {
            let mut context = context.borrow_mut();
            context.set_state_callback(Some(state_callback));
            context.connect(None, FlagSet::NOAUTOSPAWN, None).unwrap();
        }

        loop {
            if let Ok(command) = self.rx.try_recv() {
                match command {
                    Command::SetVolume(id, cv) => {
                        let context = context.borrow_mut();
                        let mut introspect = context.introspect();

                        introspect.set_sink_input_volume(id, &cv, None);
                    },
                }
            }

            match mainloop.iterate(false) {
                IterateResult::Success(_) => {},
                IterateResult::Err(e) => {
                    sender.emit(Message::Error(anyhow!("Pulse Audio: {e}]")))
                }
                IterateResult::Quit(_) => {
                    sender.emit(Message::Disconnected(None));
                    break
                },
            }
        }
    }

    fn set_volume(&self, id: u32, volume: Volume) {
        if let Volume::Pulse(cv) = volume {
            self.tx.try_send(Command::SetVolume(id, cv)).unwrap_or(());
        }
    }
}

impl <'a> From<&SinkInputInfo<'a>> for Client {
    fn from(sink_input: &SinkInputInfo<'a>) -> Self {
        let name = sink_input.proplist.get_str("application.name").unwrap_or_default();
        let description = sink_input.name.as_ref().map(Cow::to_string).unwrap_or_default();

        Client {
            id: sink_input.index,
            name,
            description,
            icon: "".to_owned(),
            volume: Volume::Pulse(sink_input.volume),
            muted: sink_input.mute,
        }
    }
}

fn subscribe_callback(sender: &Sender<Message>, context: &Context, _: Option<Facility>, op: Option<Operation>, i: u32) {
    let introspect = context.introspect();

    match op {
        Some(Operation::New) => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();
                move |info| {
                    let ListResult::Item(info) = info else {
                        return
                    };

                    sender.send(Message::New(info.into())).unwrap();
                }
            });
        },
        Some(Operation::Changed) => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();
                move |info| {
                    let ListResult::Item(info) = info else {
                        return
                    };

                    sender.send(Message::Changed(info.into())).unwrap();
                }
            });
        },
        Some(Operation::Removed) => {
            sender.send(Message::Removed(i)).unwrap();
        },
        None => {},
    }
}

fn print_state(sender: &Sender<Message>, context: &Rc<RefCell<Context>>) {
    use libpulse_binding::context::State::*;

    let state = match context.try_borrow() {
        Ok(context) => context.get_state(),
        Err(_) => return,
    };

    match state {
        Ready => {
            {
                let sender = sender.clone();
                let context = context.try_borrow().unwrap();

                context.introspect().get_sink_input_info_list(move |r| {
                    if let ListResult::Item(sink_input) = r {
                        sender.send(Message::New(sink_input.into())).unwrap();
                    }
                });
            }

            let subscribe_callback = Box::new({
                let sender = sender.clone();
                let context = context.clone();

                move |f, op, i| {
                    if let Ok(borrow) = context.try_borrow() {
                        subscribe_callback(&sender, &borrow, f, op, i);
                    }
                }
            });

            let mut context = context.borrow_mut();

            context.set_subscribe_callback(Some(subscribe_callback));
            context.subscribe(InterestMaskSet::SINK_INPUT, |_| ());
        },
        Failed => sender.emit(Message::Error(anyhow!("Pulse Audio: connection failed"))),
        Terminated => sender.emit(Message::Error(anyhow!("Pulse Audio: connection terminated"))),
        _ => {},
    }
}
