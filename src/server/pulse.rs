use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::anyhow;
use libpulse_binding::def::Retval;
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

pub struct Pulse {
    volume:     Mutex<Option<(u32, ChannelVolumes)>>,
    mute:       Mutex<Option<(u32, bool)>>,
    disconnect: AtomicBool,
}

impl Pulse {
    pub fn new() -> Self {
        Self {
            volume:     Mutex::new(None),
            mute:       Mutex::new(None),
            disconnect: AtomicBool::new(false),
        }
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

            move || state_callback(&sender, &context)
        });

        {
            let mut context = context.borrow_mut();
            context.set_state_callback(Some(state_callback));
            context.connect(None, FlagSet::NOAUTOSPAWN, None).unwrap();
        }

        let mut block = false;

        loop {
            match mainloop.iterate(block) {
                IterateResult::Success(_) => {
                    block = false;
                },
                IterateResult::Err(e) => {
                    sender.emit(Message::Error(anyhow!("Pulse Audio: {e}]")))
                }
                IterateResult::Quit(_) => break,
            }

            if let Some((id, cv)) = self.volume.try_lock().ok().and_then(|mut lock| (*lock).take()) {
                let context = context.borrow_mut();
                let mut introspect = context.introspect();
                introspect.set_sink_input_volume(id, &cv, None);

                block = true;
            }

            if let Some((id, mute)) = self.mute.try_lock().ok().and_then(|mut lock| (*lock).take()) {
                let context = context.borrow_mut();
                let mut introspect = context.introspect();
                introspect.set_sink_input_mute(id, mute, None);

                block = true;
            }

            if self.disconnect.load(Ordering::Relaxed) {
                mainloop.quit(Retval(0));

                block = true;
            };

            std::thread::sleep(Duration::from_micros(500));
        }

        context.borrow_mut().disconnect();
        sender.emit(Message::Disconnected(None));
    }

    fn disconnect(&self) {
        self.disconnect.store(true, Ordering::Relaxed);
    }

    fn set_volume(&self, id: u32, volume: Volume) {
        if let Volume::Pulse(cv) = volume {
            if let Ok(mut lock) = self.volume.lock() {
                *lock = Some((id, cv));
            }
        }
    }

    fn set_mute(&self, id: u32, flag: bool) {
        if let Ok(mut lock) = self.mute.lock() {
            *lock = Some((id, flag));
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

                    let client = Box::new(info.into());
                    sender.emit(Message::New(client));
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

                    let client = Box::new(info.into());
                    sender.emit(Message::Changed(client));
                }
            });
        },
        Some(Operation::Removed) => {
            sender.emit(Message::Removed(i));
        },
        None => {},
    }
}

fn state_callback(sender: &Sender<Message>, context: &Rc<RefCell<Context>>) {
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
                        let client = Box::new(sink_input.into());
                        sender.emit(Message::New(client));
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
