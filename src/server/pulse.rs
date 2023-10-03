use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, Arc};

use anyhow::anyhow;
use libpulse_binding::def::BufferAttr;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::{Stream, self, PeekResult};
use relm4::Sender;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::subscribe::{InterestMaskSet, Operation};
use libpulse_binding::context::introspect::SinkInputInfo;
use libpulse_binding::context::{Context, FlagSet};
use libpulse_binding::mainloop::standard::{Mainloop, IterateResult};
use libpulse_binding::proplist::Proplist;
use libpulse_binding::proplist::properties::APPLICATION_NAME;

use super::{Message, Volume, AudioServer, Client};

pub struct Pulse {
    context: Arc<Mutex<Option<Context>>>,
    peakers: Peakers,
}

#[derive(Clone)]
struct Peakers(Rc<RefCell<Vec<Rc<RefCell<Stream>>>>>);

unsafe impl Send for Peakers {}
unsafe impl Sync for Peakers {}

impl Peakers {
    fn add(&self, peaker: Rc<RefCell<Stream>>) {
        self.0.borrow_mut().push(peaker);
    }

    fn remove(&self, i: u32) {

        let mut peakers = self.0.borrow_mut();

        if let Some(pos) = peakers.iter().position(|stream| stream.borrow().get_index() == Some(i)) {
            let stream = peakers.get(pos).unwrap();
            stream.borrow_mut().set_read_callback(None);
            peakers.remove(pos);
        }
    }
}

impl Pulse {
    pub fn new() -> Self {
        Self {
            context: Arc::new(Mutex::new(None)),
            peakers: Peakers(Rc::new(RefCell::new(Vec::new()))),
            // peakers: Arc::new(Vec::new()),
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
            let peakers = self.peakers.clone();
            // let mut peakers = self.peakers.clone();

            move || state_callback(&sender, &context, &peakers)
        });

        {
            let mut lock = self.context.lock().unwrap();
            let context = lock.as_mut().unwrap();

            context.set_state_callback(Some(state_callback));
            context.connect(None, FlagSet::NOAUTOSPAWN, None).unwrap();
        }

        loop {
            match mainloop.iterate(true) {
                IterateResult::Success(_) => {},
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

fn peak_callback(peak: &Arc<AtomicU32>, stream: &mut Stream, i: u32) {
    if let Ok(PeekResult::Data(b)) = stream.peek() {
        let b = <[u8; 4]>::try_from(b).unwrap();

        peak.store(f32::from_le_bytes(b).to_bits(), Ordering::Relaxed)
    }

    stream.discard().expect("discarding peak stream data");
}

fn create_peeker(peak: Arc<AtomicU32>, context: &mut Context, i: u32) -> Rc<RefCell<Stream>> {
    static PEAK_BUF_ATTR: &BufferAttr = &BufferAttr {
        maxlength: u32::MAX, tlength:   u32::MAX,
        prebuf:    u32::MAX, minreq:    u32::MAX,
        fragsize:  4,
    };

    static PEAK_SPEC: Spec = Spec {
        channels: 1,
        format:   Format::F32le,
        rate:     144,
    };

    let mut stream = Stream::new(context, "Sink Input Peaker", &PEAK_SPEC, None).unwrap();

    stream.set_monitor_stream(i).unwrap();
    stream.connect_record(None, Some(PEAK_BUF_ATTR), stream::FlagSet::DONT_INHIBIT_AUTO_SUSPEND | stream::FlagSet::DONT_MOVE | stream::FlagSet::ADJUST_LATENCY | stream::FlagSet::PEAK_DETECT).unwrap();

    let stream = Rc::new(RefCell::new(stream));

    let peak_callback = Box::new({
        // let sender = sender.clone();
        let peak = peak.clone();
        let stream = stream.clone();

        move |_| peak_callback(&peak, &mut stream.borrow_mut(), i)
    });

    stream.borrow_mut().set_read_callback(Some(peak_callback));

    stream
}

fn subscribe_callback(sender: &Sender<Message>, context: &Arc<Mutex<Option<Context>>>, peakers: &Peakers, op: Option<Operation>, i: u32) {
    let introspect = {
        let context = context.lock().unwrap();
        let Some(context) = context.as_ref() else { return };

        context.introspect()
    };

    match op {
        Some(Operation::New) => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();
                let context = context.clone();
                let peakers = peakers.clone();

                move |info| {
                    if let ListResult::Item(info) = info {
                        let client: Box<Client> = Box::new(info.into());
                        let peak = client.peak.clone();
                        sender.emit(Message::New(client));

                        if let Ok(Some(context)) = context.lock().as_deref_mut() {
                            let peeker = create_peeker(peak, context, i);
                            peakers.add(peeker);
                        }
                    }
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
            peakers.remove(i);
        },
        None => {},
    }
}

fn state_callback(sender: &Sender<Message>, context: &Arc<Mutex<Option<Context>>>, peakers: &Peakers) {
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
                let peakers = peakers.clone();

                move |_, op, i| {
                    subscribe_callback(&sender, &context, &peakers, op, i);
                }
            });

            let info_callback = {
                let sender = sender.clone();
                let peakers = peakers.clone();
                let context = context.clone();

                move |r: ListResult<&SinkInputInfo>| {
                    if let ListResult::Item(sink_input) = r {
                        let client: Box<Client> = Box::new(sink_input.into());
                        let peak = client.peak.clone();

                        sender.emit(Message::New(client));

                        if let Ok(Some(context)) = context.lock().as_deref_mut() {
                            let peaker = create_peeker(peak, context, sink_input.index);
                            peakers.add(peaker);
                        }
                    }
                }
            };

            let mut lock = context.lock().unwrap();
            let Some(context) = lock.as_mut() else { return };

            context.introspect().get_sink_input_info_list(info_callback);

            context.set_subscribe_callback(Some(subscribe_callback));
            context.subscribe(InterestMaskSet::SINK_INPUT, |_| ());
        },
        Failed => sender.emit(Message::Error(anyhow!("Pulse Audio: connection failed"))),
        Terminated => sender.emit(Message::Error(anyhow!("Pulse Audio: connection terminated"))),
        _ => {},
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
            peak: Arc::new(AtomicU32::new(0)),
        }
    }
}
