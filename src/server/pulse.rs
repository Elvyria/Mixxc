use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicU8, Ordering}, OnceLock, Weak};
use std::thread::Thread;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{self, introspect::{SinkInfo, SinkInputInfo}, subscribe::{Facility, InterestMaskSet, Operation}, Context, State};
use libpulse_binding::def::{Retval, BufferAttr};
use libpulse_binding::mainloop::standard::Mainloop;
use libpulse_binding::proplist::{properties::APPLICATION_NAME, Proplist};
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::{Stream, self, PeekResult};
use libpulse_binding::volume::ChannelVolumes;

use parking_lot::{Mutex, MutexGuard};
use relm4::Sender;
use tokio::sync::watch;

use super::error::{Error, PulseError};
use super::{AudioServer, Client, Kind, Message, Volume};

type Pb<T> = Pin<Box<T>>;
type Peakers = Vec<Pb<Stream>>;

pub struct Pulse {
    context: Arc<Mutex<RefCell<Context>>>,
    peakers: Arc<Mutex<RefCell<Peakers>>>,
    state:   Arc<AtomicU8>,
    lock:    watch::Sender<Lock>,
    thread:  Mutex<Thread>,
}

#[repr(u8)]
#[derive(PartialEq)]
enum Lock {
    Unlocked = 0,
    Locked   = 1,
    Aquire   = 2,
}

impl Pulse {
    thread_local! {
        static MAINLOOP: RefCell<Mainloop> = RefCell::new(Mainloop::new().unwrap());
    }

    pub fn new() -> Self {
        let context = Pulse::MAINLOOP.with_borrow(|mainloop| {
            Context::new(mainloop, "Mixxc Context").unwrap()
        });

        Self {
            context: Arc::new(Mutex::new(RefCell::new(context))),
            peakers: Arc::new(Mutex::new(RefCell::new(Vec::with_capacity(8)))),
            state:   Arc::new(AtomicU8::new(0)),
            lock:    watch::channel(Lock::Unlocked).0,
            thread:  Mutex::new(std::thread::current()),
        }
    }

    #[inline]
    fn set_state(&self, state: context::State) {
        self.state.store(state as u8, Ordering::Release);
    }

    #[inline]
    fn is_connected(&self) -> bool {
        use num_traits::FromPrimitive;

        // SAFETY: State is updated only through the context state pull in a callback
        let state = unsafe { State::from_u8(self.state.load(Ordering::Acquire)).unwrap_unchecked() };

        state == State::Ready
    }

    async fn lock(&self) -> ContextRef {
        self.lock.send_replace(Lock::Aquire);
        self.lock.subscribe().wait_for(|lock| *lock == Lock::Locked).await.unwrap();

        let context = self.context.try_lock().unwrap();
        let thread = self.thread.try_lock().unwrap();

        ContextRef {
            context,
            lock: &self.lock,
            thread,
        }
    }

    fn lock_blocking(&self) -> ContextRef {
        self.lock.send_replace(Lock::Aquire);
        while *self.lock.borrow() != Lock::Locked {}

        let context = self.context.try_lock().unwrap();
        let thread = self.thread.try_lock().unwrap();

        ContextRef {
            context,
            lock: &self.lock,
            thread,
        }
    }

    fn iterate(timeout: Duration) -> Result<u32, PulseError> {
        Self::MAINLOOP.with_borrow_mut(|mainloop| {
            mainloop.prepare(timeout.into()).map_err(PulseError::from)?;
            mainloop.poll().map_err(PulseError::from)?;
            mainloop.dispatch().map_err(PulseError::from)
        })
    }
}

impl AudioServer for Pulse {
    fn connect(&self, sender: Sender<Message>) -> Result<(), Error> {
        let mut proplist = Proplist::new().unwrap();
        proplist.set_str(APPLICATION_NAME, crate::APP_NAME).unwrap();

        self.context.lock().replace(Pulse::MAINLOOP.with_borrow(|mainloop| {
            Context::new_with_proplist(mainloop, "Mixxc Context", &proplist).unwrap()
        }));

        let state_callback = Box::new({
            let context = Arc::downgrade(&self.context);
            let state = Arc::downgrade(&self.state);
            let sender = sender.clone();

            move || state_callback(&context, &state, &sender)
        });

        {
            let guard = self.context.lock();
            let mut context = guard.borrow_mut();

            // Manually calls state_callback and sets state to Connecting on success
            context.connect(None, context::FlagSet::NOAUTOSPAWN, None)
                .map_err(PulseError::from)?;

            self.set_state(State::Connecting);

            context.set_state_callback(Some(state_callback));
        }

        *self.thread.lock() = std::thread::current();

        loop {
            match Pulse::iterate(std::time::Duration::from_millis(1).into()) {
                Ok(_) => {},
                Err(PulseError::MainloopQuit) => break,
                Err(e) => sender.emit(Message::Error(e.into())),
            };

            if self.lock.send_if_modified(|lock| {
                match *lock == Lock::Aquire {
                    true => {
                        *lock = Lock::Locked;
                        true
                    }
                    false => false,
                }
            }) {
                std::thread::park();

                if self.state.load(Ordering::Acquire) == State::Terminated as u8 {
                    Pulse::MAINLOOP.with_borrow_mut(|mainloop| mainloop.quit(Retval(0)));
                }
            }
        }

        self.peakers.lock().borrow_mut().clear();

        Ok(())
    }

    fn disconnect(&self) {
        let guard = self.lock_blocking();
        let mut context = guard.borrow_mut();

        context.set_state_callback(None);
        context.disconnect();

        // Context::disconnect manually calls state_callback and sets state to Terminated
        self.set_state(State::Terminated);
    }

    async fn request_software(&self, sender: Sender<Message>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let input_callback = {
            let context = Arc::downgrade(&self.context);
            let peakers = Arc::downgrade(&self.peakers);

            move |info: ListResult<&SinkInputInfo>| {
                add_sink_input(info, &context, &sender, &peakers);
            }
        };

        let guard = self.lock().await;
        let context = guard.borrow();
        context.introspect().get_sink_input_info_list(input_callback);

        Ok(())
    }

    async fn request_master(&self, sender: Sender<Message>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let sink_callback = move |info: ListResult<&SinkInfo>| {
            if let ListResult::Item(info) = info {
                let client: Box<Client> = Box::new(info.into());
                sender.emit(Message::New(client));
            }
        };

        let guard = self.lock().await;
        let context = guard.borrow();
        context.introspect().get_sink_info_by_index(0, sink_callback);

        Ok(())
    }

    async fn subscribe(&self, plan: Kind, sender: Sender<Message>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let subscribe_callback = Box::new({
            let context = Arc::downgrade(&self.context);
            let peakers = Arc::downgrade(&self.peakers);

            move |facility, op, i| {
                subscribe_callback(&sender, &context, &peakers, facility, op, i)
            }
        });

        let mut mask = InterestMaskSet::NULL;
        if plan.contains(Kind::Software) { mask |= InterestMaskSet::SINK_INPUT; }
        if plan.contains(Kind::Hardware) { mask |= InterestMaskSet::SINK;       }

        let guard = self.lock().await;
        let mut context = guard.borrow_mut();

        context.set_subscribe_callback(Some(subscribe_callback));
        context.subscribe(mask, |_| ());

        Ok(())
    }

    fn set_volume(&self, id: u32, kind: Kind, volume: Volume) {
        if self.is_connected() {
            let guard = self.context.lock();
            let context = guard.borrow();
            let mut introspect = context.introspect();

            match kind {
                k if k.contains(Kind::Out | Kind::Software) => {
                    introspect.set_sink_input_volume(id, &volume.into(), None);
                },
                k if k.contains(Kind::Out | Kind::Hardware) => {
                    introspect.set_sink_volume_by_index(id, &volume.into(), None);
                },
                _ => {}
            };
        }
    }

    fn set_mute(&self, id: u32, kind: Kind, flag: bool) {
        if self.is_connected() {
            let guard = self.context.lock();
            let context = guard.borrow();
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

fn add_sink_input(info: ListResult<&SinkInputInfo>, context: &Weak<Mutex<RefCell<Context>>>, sender: &Sender<Message>, peakers: &Weak<Mutex<RefCell<Peakers>>>)
{
    let Some(context) = context.upgrade() else { return };
    let Some(peakers) = peakers.upgrade() else { return };

    if let ListResult::Item(info) = info {
        if !info.has_volume { return }

        let client: Box<Client> = Box::new(info.into());
        let id = client.id;

        sender.emit(Message::New(client));

        let guard = context.lock();
        let mut context = guard.borrow_mut();

        if let State::Ready = context.get_state() {
            if let Some(p) = create_peeker(&mut context, sender, id) {
                let guard = peakers.lock();
                let mut peakers = guard.borrow_mut();

                peakers.push(p)
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

    const PEAK_BUF_ATTR: &BufferAttr = &BufferAttr {
        maxlength: std::mem::size_of::<f32>() as u32,
        fragsize:  std::mem::size_of::<f32>() as u32,

        prebuf: 0, minreq: 0, tlength: 0,
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

    const FLAGS: FlagSet = FlagSet::PEAK_DETECT
            .union(FlagSet::DONT_INHIBIT_AUTO_SUSPEND)
            .union(FlagSet::PASSTHROUGH)
            .union(FlagSet::START_UNMUTED);

    let mut stream = Stream::new(context, "Mixxc Peaker", spec, None)?;
    stream.set_monitor_stream(i).ok()?;
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

fn subscribe_callback(sender: &Sender<Message>, context: &Weak<Mutex<RefCell<Context>>>, peakers: &Weak<Mutex<RefCell<Peakers>>>, facility: Option<Facility>, op: Option<Operation>, i: u32) {
    let Some(context) = context.upgrade() else { return };
    let Some(op) = op else { return };

    let guard = context.lock();
    let _context = guard.borrow_mut();
    let introspect = _context.introspect();

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
                let context = Arc::downgrade(&context);
                let peakers = peakers.clone();

                move |info| add_sink_input(info, &context, &sender, &peakers)
            });
        },
        Operation::Removed => {
            if let Some(peakers) = peakers.upgrade() {
                let guard = peakers.lock();
                let mut peakers = guard.borrow_mut();

                if let Some(pos) = peakers.iter().position(|stream| stream.get_index() == Some(i)) {
                    peakers.remove(pos);
                }
            }

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

fn state_callback(context: &Weak<Mutex<RefCell<Context>>>, state: &Weak<AtomicU8>, sender: &Sender<Message>) {
    let Some(context) = context.upgrade() else { return };

    let guard = context.lock();
    let new_state = guard.borrow().get_state();

    if let Some(state) = state.upgrade() {
        state.store(new_state as u8, Ordering::Release);
    }

    match new_state {
        State::Ready => sender.emit(Message::Ready),
        State::Failed => {
            let e = PulseError::from(guard.borrow().errno());
            sender.emit(Message::Disconnected(Some(e.into())));
        },
        State::Terminated => sender.emit(Message::Disconnected(None)),
        _ => {},
    }
}

struct ContextRef<'a> {
    context: MutexGuard<'a, RefCell<Context>>,
    lock: &'a watch::Sender<Lock>,
    thread: MutexGuard<'a, Thread>,
}

impl Drop for ContextRef<'_> {
    #[inline]
    fn drop(&mut self) {
        self.lock.send_replace(Lock::Unlocked);
        self.thread.unpark();
    }
}

impl<'a> Deref for ContextRef<'a> {
    type Target = RefCell<Context>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

struct Duration(std::time::Duration);

impl From<std::time::Duration> for Duration {
    #[inline]
    fn from(d: std::time::Duration) -> Self { Self(d) }
}

impl From<Duration> for Option<libpulse_binding::time::MicroSeconds> {
    #[inline]
    fn from(d: Duration) -> Self {
        match d.0.is_zero() {
            false => Some(libpulse_binding::time::MicroSeconds(d.0.as_micros() as u64)),
            true => None,
        }
    }
}

impl From<Volume> for ChannelVolumes {
    fn from(v: Volume) -> Self {
        let mut cv = ChannelVolumes::default();
        cv.set_len(v.inner.len() as u8);
        cv.get_mut().copy_from_slice(unsafe {
            std::mem::transmute::<&[u32], &[libpulse_binding::volume::Volume]>(&v.inner)
        });
        cv
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
