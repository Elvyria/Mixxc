use std::borrow::Cow;
use std::sync::{Mutex, Arc};

use anyhow::anyhow;
use relm4::Sender;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{InterestMaskSet, Facility, Operation};
use libpulse_binding::context::introspect::SinkInputInfo;
use libpulse_binding::context::{Context, FlagSet};
use libpulse_binding::mainloop::standard::{Mainloop, IterateResult};
use libpulse_binding::proplist::Proplist;
use libpulse_binding::proplist::properties::APPLICATION_NAME;

use super::{Message, Volume, AudioServer, Client};

pub struct Pulse {
    context: Arc<Mutex<Option<Context>>>
}

impl Pulse {
    pub fn new() -> Self {
        Self {
            context: Arc::new(Mutex::new(None)),
        }
    }
}

impl AudioServer for Pulse {
    fn connect(&self, sender: Sender<Message>) {
        let mut proplist = Proplist::new().unwrap();
        proplist.set_str(APPLICATION_NAME, crate::APP_NAME).unwrap();

        let mut mainloop = Mainloop::new().unwrap();

        let context = Context::new_with_proplist(&mainloop, "Mixxc Context", &proplist).unwrap();

        {
            let mut lock = self.context.lock().unwrap();
            lock.replace(context);
        }

        let state_callback = Box::new({
            let context = self.context.clone();
            let sender = sender.clone();

            move || state_callback(&sender, &context)
        });

        {
            let mut lock = self.context.lock().unwrap();
            let context = lock.as_mut().unwrap();

            context.set_state_callback(Some(state_callback));
            context.connect(None, FlagSet::NOAUTOSPAWN, None).unwrap();
        }

        loop {
            match mainloop.iterate(true) {
                IterateResult::Success(_) => { },
                IterateResult::Err(e) => {
                    sender.emit(Message::Error(anyhow!("Pulse Audio: {e}]")))
                }
                IterateResult::Quit(_) => break,
            }
        }

        self.disconnect();
        sender.emit(Message::Disconnected(None));
    }

    fn disconnect(&self) {
        let Ok(mut lock) = self.context.lock() else {
            return
        };

        if let Some(mut context) = lock.take() {
            context.disconnect();
        }
    }

    fn set_volume(&self, id: u32, volume: Volume) {
        let Volume::Pulse(cv) = volume else {
            return
        };

        if let Ok(Some(context)) = self.context.lock().as_deref() {
            context.introspect().set_sink_input_volume(id, &cv, None);
        }
    }

    fn set_mute(&self, id: u32, flag: bool) {
        if let Ok(Some(context)) = self.context.lock().as_deref() {
            context.introspect().set_sink_input_mute(id, flag, None);
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

fn state_callback(sender: &Sender<Message>, context: &Arc<Mutex<Option<Context>>>) {
    use libpulse_binding::context::State::*;

    let Some(state) =
        context.try_lock()
        .ok()
        .and_then(|lock| lock.as_ref().map(Context::get_state)) else {
        return
    };

    match state {
        Ready => {
            let subscribe_callback = Box::new({
                let sender = sender.clone();
                let context = context.clone();

                move |f, op, i| {
                    if let Ok(Some(context)) = context.lock().as_deref() {
                        subscribe_callback(&sender, context, f, op, i);
                    }
                }
            });

            let Ok(mut lock) = context.lock() else {
                return
            };

            let Some(context) = lock.as_mut() else {
                return
            };

            let sender = sender.clone();

            context.introspect().get_sink_input_info_list(move |r| {
                if let ListResult::Item(sink_input) = r {
                    let client = Box::new(sink_input.into());
                    sender.emit(Message::New(client));
                }
            });

            context.set_subscribe_callback(Some(subscribe_callback));
            context.subscribe(InterestMaskSet::SINK_INPUT, |_| ());
        },
        Failed => sender.emit(Message::Error(anyhow!("Pulse Audio: connection failed"))),
        Terminated => sender.emit(Message::Error(anyhow!("Pulse Audio: connection terminated"))),
        _ => {},
    }
}
