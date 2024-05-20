use std::borrow::Cow;
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Mutex, Arc, OnceLock};

use libpulse_binding::context;
use relm4::Sender;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{introspect::{SinkInfo, SinkInputInfo}, subscribe::{Facility, InterestMaskSet, Operation}, Context};
use libpulse_binding::def::BufferAttr;
use libpulse_binding::mainloop::standard::{Mainloop, IterateResult};
use libpulse_binding::proplist::{properties::APPLICATION_NAME, Proplist};
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::{Stream, self, PeekResult};

use super::error::{Error, PulseError};

use super::{AudioServer, Client, Kind, Message, Volume};

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
    fn connect(&self, sender: Sender<Message>) -> Result<(), Error> {
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

            move || state_callback(&context, &sender)
        });

        {
            let mut lock = self.context.try_lock().unwrap();
            let context = lock.as_mut().unwrap();

            context.set_state_callback(Some(state_callback));

            context.connect(None, context::FlagSet::NOAUTOSPAWN, None)
                .map_err(PulseError::from)?;
        }

        loop {
            match mainloop.iterate(true) {
                IterateResult::Success(_) => {},
                IterateResult::Err(e) => {
                    let e: Error = PulseError::from(e).into();
                    sender.emit(Message::Error(e));
                }
                IterateResult::Quit(_) => break,
            }
        }

        (*self.peakers.0).borrow_mut().clear();

        self.context.lock()
            .as_deref_mut()
            .map(Option::take)
            .unwrap();

        sender.emit(Message::Disconnected(None));

        Ok(())
    }

    fn disconnect(&self) {
        let Ok(mut lock) = self.context.lock() else {
            return
        };

        match lock.take() {
            Some(mut context) => {
                context.disconnect();
            }
            None => {
                (*self.peakers.0).borrow_mut().clear();
            },
        }
    }

    fn request_software(&self, sender: Sender<Message>) -> Result<(), Error> {
        let input_callback = {
            let peakers = self.peakers.clone();
            let context = self.context.clone();

            move |info: ListResult<&SinkInputInfo>| {
                add_sink_input(info, &context, &sender, &peakers);
            }
        };

        match self.context.lock().as_deref_mut() {
            Ok(Some(context)) => {
                context.introspect().get_sink_input_info_list(input_callback);

                Ok(())
            }
            _ => Err(PulseError::Context.into()),
        }
    }

    fn request_master(&self, sender: Sender<Message>) -> Result<(), Error> {
        let sink_callback = {
            let peakers = self.peakers.clone();
            let context = self.context.clone();

            move |info: ListResult<&SinkInfo>| {
                add_sink_input(info, &context, &sender, &peakers);
            }
        };

        match self.context.lock().as_deref_mut() {
            Ok(Some(context)) => {
                context.introspect().get_sink_info_by_index(0, sink_callback);

                Ok(())
            }
            _ => Err(PulseError::Context.into()),
        }
    }

    fn subscribe(&self, plan: Kind, sender: Sender<Message>) -> Result<(), Error> {
        let subscribe_callback = Box::new({
            let sender = sender.clone();
            let context = self.context.clone();
            let peakers = self.peakers.clone();

            move |facility, op, i| subscribe_callback(&sender, &context, &peakers, facility, op, i)
        });

        let mut mask = InterestMaskSet::NULL;
        if plan.contains(Kind::Software) { mask |= InterestMaskSet::SINK_INPUT; }
        if plan.contains(Kind::Hardware) { mask |= InterestMaskSet::SINK;       }

        match self.context.lock().as_deref_mut() {
            Ok(Some(context)) => {
                context.set_subscribe_callback(Some(subscribe_callback));
                context.subscribe(mask, |_| ());

                Ok(())
            }
            _ => Err(PulseError::Context.into()),
        }
    }

    fn set_volume(&self, id: u32, kind: Kind, volume: Volume) {
        let mut cv = libpulse_binding::volume::ChannelVolumes::default();
        cv.set_len(volume.inner.len() as u8);
        cv.get_mut().copy_from_slice(unsafe {
            std::mem::transmute::<&[u32], &[libpulse_binding::volume::Volume]>(&volume.inner)
        });

        if let Ok(Some(context)) = self.context.try_lock().as_deref() {
            let mut introspect = context.introspect();

            match kind {
                k if k.contains(Kind::Out | Kind::Software) => {
                    introspect.set_sink_input_volume(id, &cv, None);
                },
                k if k.contains(Kind::Out | Kind::Hardware) => {
                    introspect.set_sink_volume_by_index(id, &cv, None);
                },
                _ => {}
            };
        }
    }

    fn set_mute(&self, id: u32, kind: Kind, flag: bool) {
        if let Ok(Some(context)) = self.context.try_lock().as_deref() {
            let mut introspect = context.introspect();

            match kind {
                k if k.contains(Kind::Out | Kind::Software) => {
                    introspect.set_sink_input_mute(id, flag, None);
                },
                k if k.contains(Kind::Out | Kind::Hardware) => {
                    introspect.set_sink_mute_by_index(id, flag, None);
                },
                _ => {}
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

fn peak_callback(stream: &mut Stream, sender: &Sender<Message>, i: u32) {
    match stream.peek() {
        Ok(PeekResult::Data(b)) => {
            let bytes: [u8; 4] = unsafe { b.try_into().unwrap_unchecked() };
            let peak: f32 = f32::from_ne_bytes(bytes);

            if peak != 0.0 { sender.emit(Message::Peak(i, peak)); }
        }
        Ok(PeekResult::Hole(_)) => {},
        _ => return,
    }

    let _ = stream.discard();
}

fn create_peeker(context: &mut Context, sender: &Sender<Message>, i: u32) -> Option<Pb<Stream>> {
    use stream::FlagSet;

    // TODO: set_monitor_stream requires the real sink id, instead of 0,
    // and needs to be destroyed/created if *default* device was changed.
    // For now just don't create peaker that spams 0's.
    if i == 0 { return None }

    const PEAK_BUF_ATTR: &BufferAttr = &BufferAttr {
        maxlength: 0, tlength:   0,
        prebuf:    0, minreq:    0,
        fragsize:  std::mem::size_of::<f32>() as u32,
    };

    static PEAK_SPEC: OnceLock<Spec> = OnceLock::new();

    let spec = PEAK_SPEC.get_or_init(|| {
        Spec {
            channels: 1,
            format:   Format::FLOAT32NE,
            rate:     {
                std::env::var("PULSE_PEAK_RATE").ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(30)
            }
        }
    });

    let mut stream = Stream::new(context, "Mixxc Peaker", spec, None)?;
    stream.set_monitor_stream(i).ok()?;

    const FLAGS: FlagSet = FlagSet::PEAK_DETECT
            .union(FlagSet::ADJUST_LATENCY)
            .union(FlagSet::START_UNMUTED);

    stream.connect_record(None, Some(PEAK_BUF_ATTR), FLAGS).ok()?;

    let mut stream = Box::pin(stream);

    let peak_callback = Box::new({
        let sender = sender.clone();
        let stream: &mut Stream = unsafe { &mut *(stream.as_mut().get_mut() as *mut Stream) };

        move |_| peak_callback(stream, &sender, i)
    });

    stream.set_read_callback(Some(peak_callback));

    Some(stream)
}

fn subscribe_callback(sender: &Sender<Message>, context: &Arc<Mutex<Option<Context>>>, peakers: &Peakers, facility: Option<Facility>, op: Option<Operation>, i: u32) {
    let Some(op) = op else { return };

    let introspect = match context.lock().as_deref() {
        Ok(Some(context)) => context.introspect(),
        _ => return,
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
            peakers.remove(i);
            sender.emit(Message::Removed(i));
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

fn state_callback(context: &Arc<Mutex<Option<Context>>>, sender: &Sender<Message>) {
    use libpulse_binding::context::State::*;

    // Never attempt to use Mutex::lock here, it will deadlock while connecting.
    if let Ok(Some(context)) = context.try_lock().as_deref_mut() {
        if let Ready = context.get_state() {
            sender.emit(Message::Ready)
        }
    }
}

impl Volume {
    fn pulse_linear(&self) -> f64 {
        use libpulse_binding::volume::{Volume, VolumeLinear};

        let max = *self.inner.iter().max().unwrap_or(&0);
        VolumeLinear::from(Volume(max)).0
    }

    fn set_pulse_linear(&mut self, v: f64) {
        use libpulse_binding::volume::{Volume, VolumeLinear};

        let v = Volume::from(VolumeLinear(v)).0;
        let max = *self.inner.iter().max().unwrap();

        if max > Volume::MUTED.0 {
            self.inner.iter_mut()
                .for_each(|i| *i = ((*i as u64 * v as u64 / max as u64) as u32).clamp(Volume::MUTED.0, Volume::MAX.0));
        }
        else { self.inner.fill(v); }
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
            inner: {
                let levels: &[u32] = unsafe {
                    use libpulse_binding::volume::Volume;

                    std::mem::transmute::<&[Volume], &[u32]>(sink_input.volume.get())
                };

                smallvec::SmallVec::from_slice(&levels[..sink_input.volume.len() as usize])
            },
            percent: &Volume::pulse_linear,
            set_percent: &Volume::set_pulse_linear,
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
            kind: Kind::Out | Kind::Software,
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
            inner: {
                let levels: &[u32] = unsafe {
                    use libpulse_binding::volume::Volume;

                    std::mem::transmute::<&[Volume], &[u32]>(sink.volume.get())
                };
                smallvec::SmallVec::from_slice(&levels[..sink.volume.len() as usize])
            },
            percent: &|v: &Volume| {
                use libpulse_binding::volume::Volume;

                *v.inner.iter().max().unwrap() as f64 / Volume::NORMAL.0 as f64
            },
            set_percent: &|v: &mut Volume, p: f64| {
                v.inner.fill((libpulse_binding::volume::Volume::NORMAL.0 as f64 * p) as u32);
            },
        };

        Client {
            id: 0,
            name: "Master".to_owned(),
            description,
            icon: None,
            volume,
            max_volume: 2.55,
            muted: sink.mute,
            corked: false,
            kind: Kind::Out | Kind::Hardware,
        }
    }
}
