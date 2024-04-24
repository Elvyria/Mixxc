use std::borrow::Cow;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Mutex, Arc, OnceLock};

use anyhow::anyhow;
use relm4::Sender;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::introspect::{SinkInfo, SinkInputInfo};
use libpulse_binding::context::subscribe::{Facility, InterestMaskSet, Operation};
use libpulse_binding::context::{Context, FlagSet};
use libpulse_binding::def::BufferAttr;
use libpulse_binding::mainloop::standard::{Mainloop, IterateResult};
use libpulse_binding::proplist::Proplist;
use libpulse_binding::proplist::properties::APPLICATION_NAME;
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::{Stream, self, PeekResult};

use super::{AudioServer, Client, Message, RawVolume, Volume};

pub struct Pulse {
    context: Arc<Mutex<Option<Context>>>,
    peakers: Peakers,
}

type Pb<T> = Pin<Box<T>>;

#[derive(Clone)]
struct Peakers(Rc<RefCell<Vec<Pb<Stream>>>>);

unsafe impl Send for Peakers {}
unsafe impl Sync for Peakers {}

impl Peakers {
    fn add(&self, peaker: Pb<Stream>) {
        (*self.0).borrow_mut().push(peaker);
    }

    fn remove(&self, i: u32) {
        let mut peakers = (*self.0).borrow_mut();

        if let Some(pos) = peakers.iter().position(|stream| stream.get_index() == Some(i)) {
            let stream = peakers.get_mut(pos).unwrap();
            stream.set_read_callback(None);
            peakers.remove(pos);
        }
    }
}

impl Pulse {
    pub fn new() -> Self {
        Self {
            context: Arc::new(Mutex::new(None)),
            peakers: Peakers(Rc::new(RefCell::new(Vec::new()))),
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

            move || state_callback(&context, &peakers, &sender)
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
                    sender.emit(Message::Error(anyhow!("Pulse Audio: {e}")))
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
        #[allow(irrefutable_let_patterns)]
        let RawVolume::Pulse(cv) = volume.inner else {
            return
        };

        if let Ok(Some(context)) = self.context.lock().as_deref() {
            match id {
                super::id::MASTER => context.introspect().set_sink_volume_by_index(0, &cv, None),
                _ => context.introspect().set_sink_input_volume(id, &cv, None),
            };
        }
    }

    fn set_mute(&self, id: u32, flag: bool) {
        if let Ok(Some(context)) = self.context.lock().as_deref() {
            match id {
                super::id::MASTER => context.introspect().set_sink_mute_by_index(0, flag, None),
                _ => context.introspect().set_sink_input_mute(id, flag, None),
            };
        }
    }
}

fn add_sink_input<C>(info: ListResult<C>, context: &Arc<Mutex<Option<Context>>>, sender: &Sender<Message>, peakers: &Peakers)
where
    C: Into<Client>
{
    if let ListResult::Item(info) = info {
        let client: Box<Client> = Box::new(info.into());
        let id = client.id;

        sender.emit(Message::New(client));

        if let Ok(Some(context)) = context.lock().as_deref_mut() {
            if let Some(p) = create_peeker(context, sender, id) {
                peakers.add(p)
            }
        }
    }
}

const FRAG_SIZE: u32 = 4;

fn peak_callback(stream: &mut Stream, sender: &Sender<Message>, i: u32) {
    match stream.peek() {
        Ok(PeekResult::Data(b)) => {
            #[allow(clippy::assertions_on_constants)]
            const _: () = debug_assert!(FRAG_SIZE == 4);

            let peak: f32 = unsafe { *(b.as_ptr() as *const _) };

            sender.emit(Message::Peak(i, peak));
        }
        Ok(PeekResult::Hole(_)) => {},
        _ => return,
    }

    let _ = stream.discard();
}

fn create_peeker(context: &mut Context, sender: &Sender<Message>, i: u32) -> Option<Pb<Stream>> {
    // TODO: set_monitor_stream requires the real sink id, instead of 0,
    // and needs to be destroyed/created if *default* device was changed.
    // For now just don't create peaker that spams 0's.
    if i == super::id::MASTER { return None }

    static PEAK_BUF_ATTR: &BufferAttr = &BufferAttr {
        maxlength: 0, tlength:   0,
        prebuf:    0, minreq:    0,
        fragsize:  FRAG_SIZE,
    };

    let mut peak_spec = Spec {
        channels: 1,
        format:   Format::F32le,
        rate:     0,
    };

    static PEAK_RATE: OnceLock<u32> = OnceLock::new();
    peak_spec.rate = *PEAK_RATE.get_or_init(|| {
        std::env::var("PULSE_PEAK_RATE").ok().and_then(|s| s.parse().ok()).unwrap_or(30)
    });

    let mut stream = Stream::new(context, "Sink Input Peaker", &peak_spec, None)?;

    let flags: stream::FlagSet = stream::FlagSet::PEAK_DETECT | stream::FlagSet::ADJUST_LATENCY | stream::FlagSet::START_UNMUTED;

    stream.set_monitor_stream(i).unwrap();
    stream.connect_record(None, Some(PEAK_BUF_ATTR), flags).unwrap();

    let mut stream = Box::pin(stream);

    let peak_callback = Box::new({
        let sender = sender.clone();
        let stream: &mut Stream = unsafe { &mut *(stream.as_mut().get_mut() as *mut _) };

        move |_| peak_callback(stream, &sender, i)
    });

    stream.set_read_callback(Some(peak_callback));

    Some(stream)
}

fn subscribe_callback(sender: &Sender<Message>, context: &Arc<Mutex<Option<Context>>>, peakers: &Peakers, facility: Option<Facility>, op: Option<Operation>, i: u32) {
    let Some(op) = op else { return };

    let introspect = {
        let context = context.lock().unwrap();
        let Some(context) = context.as_ref() else { return };

        context.introspect()
    };

    if let Some(Facility::Sink) = facility {
        introspect.get_sink_info_by_index(0, {
            let sender = sender.clone();

            move |info| {
                if let ListResult::Item(info) = info {
                    let client = Box::new(info.into());
                    sender.emit(Message::Changed(client));
                };
            }
        });

        return
    }

    match op {
        Operation::New => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();
                let context = context.clone();
                let peakers = peakers.clone();

                move |info: ListResult<&SinkInputInfo>| add_sink_input(info, &context, &sender, &peakers)
            });
        },
        Operation::Removed => {
            sender.emit(Message::Removed(i));
            peakers.remove(i);
        },
        Operation::Changed => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();

                move |info| {
                    if let ListResult::Item(info) = info {
                        let client = Box::new(info.into());
                        sender.emit(Message::Changed(client));
                    };
                }
            });
        },
    }
}

fn state_callback(context: &Arc<Mutex<Option<Context>>>, peakers: &Peakers, sender: &Sender<Message>) {
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

                move |facility, op, i| subscribe_callback(&sender, &context, &peakers, facility, op, i)
            });

            let input_callback = {
                let sender = sender.clone();
                let peakers = peakers.clone();
                let context = context.clone();

                let mut ready = false;

                move |info: ListResult<&SinkInputInfo>| {
                    add_sink_input(info, &context, &sender, &peakers);

                    if !ready {
                        ready = sender.send(Message::Ready).is_ok();
                    }
                }
            };

            let sink_callback = {
                let sender = sender.clone();
                let peakers = peakers.clone();
                let context = context.clone();

                move |info: ListResult<&SinkInfo>| {
                    add_sink_input(info, &context, &sender, &peakers);
                    sender.emit(Message::Ready);
                }
            };

            let mut lock = context.lock().unwrap();
            let Some(context) = lock.as_mut() else { return };

            context.introspect().get_sink_info_by_index(0, sink_callback);
            context.introspect().get_sink_input_info_list(input_callback);

            context.set_subscribe_callback(Some(subscribe_callback));
            context.subscribe(InterestMaskSet::SINK_INPUT | InterestMaskSet::SINK, |_| ());
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
        let icon = sink_input.proplist.get_str("application.icon_name");

        // This would be the correct approach, but things get weird after 255%
        // static VOLUME_MAX: OnceLock<f64> = OnceLock::new();
        // let max = *VOLUME_MAX.get_or_init(|| VolumeLinear::from(libpulse_binding::volume::Volume::ui_max()).0);

        let volume = Volume {
            inner: RawVolume::Pulse(sink_input.volume),
            percent: &RawVolume::linear,
            set_percent: &RawVolume::set_linear,
        };

        Client {
            id: sink_input.index,
            name,
            description,
            icon,
            volume,
            max_volume: 2.55,
            muted: sink_input.mute,
            corked: sink_input.corked,
        }
    }
}

impl <'a> From<&SinkInfo<'a>> for Client {
    fn from(sink: &SinkInfo<'a>) -> Self {
        let description = sink.active_port
            .as_ref()
            .and_then(|port| port.description.to_owned())
            .unwrap_or_default()
            .to_string();

        let volume = Volume {
            inner: RawVolume::Pulse(sink.volume),
            percent: &|v: &RawVolume| {
                use libpulse_binding::volume::Volume;

                #[allow(irrefutable_let_patterns)]
                if let RawVolume::Pulse(v) = v {
                    return v.avg().0 as f64 / Volume::NORMAL.0 as f64
                }

                unreachable!()
            },
            set_percent: &|v: &mut RawVolume, p: f64| {
                use libpulse_binding::volume::Volume;

                #[allow(irrefutable_let_patterns)]
                if let RawVolume::Pulse(v) = v {
                    v.set(v.len(), Volume((Volume::NORMAL.0 as f64 * p) as u32));
                }
            },
        };

        Client {
            id: super::id::MASTER,
            name: "Master".to_owned(),
            description,
            icon: None,
            volume,
            max_volume: 2.55,
            muted: sink.mute,
            corked: false,
        }
    }
}
