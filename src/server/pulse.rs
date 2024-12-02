use std::borrow::Cow;
use std::cell::RefCell;
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicU8, Ordering}, OnceLock, Weak};
use std::thread::Thread;

use libpulse_binding::callbacks::ListResult;
use libpulse_binding::context::{self, introspect::{Introspector, SinkInfo, SinkInputInfo}, subscribe::{Facility, InterestMaskSet, Operation}, Context, State};
use libpulse_binding::def::{BufferAttr, PortAvailable, Retval};
use libpulse_binding::mainloop::standard::Mainloop;
use libpulse_binding::proplist::{properties::APPLICATION_NAME, Proplist};
use libpulse_binding::sample::{Format, Spec};
use libpulse_binding::stream::{Stream, self, PeekResult};
use libpulse_binding::volume::ChannelVolumes;

use derive_more::derive::{Deref, From};
use parking_lot::{Mutex, MutexGuard};
use smallvec::SmallVec;

use tokio::sync::watch;

use super::error::{Error, PulseError};
use super::{AudioServer, Kind, Message, MessageClient, MessageOutput, Output, OutputClient, Sender, Volume, VolumeLevels};

const DEFAULT_PEAK_RATE: u32 = 30;

type Pb<T> = Pin<Box<T>>;
type Peakers = Vec<Pb<Stream>>;

type WeakContext = Weak<Mutex<RefCell<Context>>>;
type WeakPeakers = Weak<Mutex<RefCell<Peakers>>>;

pub struct Pulse {
    context: Arc<Mutex<RefCell<Context>>>,
    peakers: Arc<Mutex<RefCell<Peakers>>>,
    state:   Arc<AtomicU8>,
    lock:    watch::Sender<Lock>,
    thread:  Mutex<Thread>,
    running: Mutex<()>,
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
            running:      Mutex::new(()),
        }
    }

    #[inline]
    fn set_state(&self, state: context::State) {
        self.state.store(state as u8, Ordering::Release);
    }

    #[inline]
    fn is_connected(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Ready as u8
    }

    #[inline]
    fn is_terminated(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Terminated as u8
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
        while *self.lock.borrow() != Lock::Locked { std::hint::spin_loop(); }

        let context = self.context.try_lock().unwrap();
        let thread = self.thread.try_lock().unwrap();

        ContextRef {
            context,
            lock: &self.lock,
            thread,
        }
    }

    fn iterate(timeout: &Duration) -> Result<u32, PulseError> {
        Self::MAINLOOP.with_borrow_mut(|mainloop| {
            mainloop.prepare(timeout.into()).map_err(PulseError::from)?;
            mainloop.poll().map_err(PulseError::from)?;
            mainloop.dispatch().map_err(PulseError::from)
        })
    }

    fn is_locked(&self) -> bool {
        self.lock.send_if_modified(|lock| {
            match *lock == Lock::Aquire {
                true => {
                    *lock = Lock::Locked;
                    true
                }
                false => false,
            }
        })
    }

    fn quit() {
        Self::MAINLOOP.with_borrow_mut(|mainloop| mainloop.quit(Retval(0)));
    }
}

impl AudioServer for Pulse {
    fn connect(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error> {
        if self.is_connected() {
            return Err(Error::AlreadyConnected)
        }

        let mut proplist = Proplist::new().unwrap();
        proplist.set_str(APPLICATION_NAME, crate::APP_NAME).unwrap();

        self.context.lock().replace(Pulse::MAINLOOP.with_borrow(|mainloop| {
            Context::new_with_proplist(mainloop, "Mixxc Context", &proplist).unwrap()
        }));

        let sender: Sender<Message> = sender.into();

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

        let timeout = std::time::Duration::from_millis(1).into();
        let _running = self.running.lock();

        loop {
            std::hint::spin_loop();

            match Pulse::iterate(&timeout) {
                Ok(_) => {},
                Err(PulseError::MainloopQuit) => break,
                Err(e) => sender.emit(Message::Error(e.into())),
            };

            if self.is_locked() {
                std::thread::park();

                if self.is_terminated() {
                    Pulse::quit()
                }
            }
        }

        self.peakers.lock().borrow_mut().clear();

        Ok(())
    }

    fn disconnect(&self) {
        {
            let guard = self.lock_blocking();
            let mut context = guard.borrow_mut();

            context.set_state_callback(None);
            context.disconnect();

            // Context::disconnect manually calls state_callback and sets state to Terminated
            self.set_state(State::Terminated);
        }

        let _running = self.running.lock();
    }

    async fn request_software(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let sender = sender.into();

        let input_callback = {
            let context = Arc::downgrade(&self.context);
            let peakers = Arc::downgrade(&self.peakers);

            move |info: ListResult<&SinkInputInfo>| {
                add_sink_input(info, &context, &sender, &peakers);
            }
        };

        let context = self.lock().await;
        context.introspect().get_sink_input_info_list(input_callback);

        Ok(())
    }

    async fn request_outputs(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let sender = sender.into();

        let sink_info_callback = move |info: ListResult<&SinkInfo>| {
            let ListResult::Item(info) = info else {
                return
            };

            let Some(output_name) = &info.name else {
                let e = PulseError::NamelessSink(info.index).into();
                sender.emit(Message::Error(e));

                return;
            };

            let ports = info.ports.iter()
                .filter(|p| p.available != PortAvailable::No);

            for port in ports {
                let Some(port_name) = &port.name else {
                    let e = PulseError::NamelessPort(info.index).into();
                    sender.emit(Message::Error(e));

                    continue;
                };

                let output = Output {
                    name: output_name.to_string(),
                    port: port_name.to_string(),
                    master: false,
                };

                let msg: Message = MessageOutput::New(output).into();
                sender.emit(msg);
            }
        };

        let guard = self.lock().await;
        let introspect = guard.introspect();

        introspect.get_sink_info_list(sink_info_callback);

        Ok(())
    }

    async fn request_master(&self, sender: impl Into<Sender<Message>>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let sender = sender.into();

        let sink_callback = move |info: ListResult<&SinkInfo>| {
            if let ListResult::Item(info) = info {
                let client: Box<OutputClient> = Box::new(info.into());
                let msg: Message = MessageClient::New(client).into();

                sender.emit(msg);

                let Some(output_name) = info.name.as_ref() else {
                    let e = PulseError::NamelessSink(info.index).into();
                    sender.emit(Message::Error(e));

                    return
                };

                let Some(port_name) = info.active_port.as_ref().and_then(|p| p.name.as_ref()) else {
                    return
                };

                let output = Output {
                    name: output_name.to_string(),
                    port: port_name.to_string(),
                    master: true,
                };

                let msg: Message = MessageOutput::Master(output).into();
                sender.emit(msg)
            }
        };

        let context = self.lock().await;
        context.introspect().get_sink_info_by_index(0, sink_callback);

        Ok(())
    }

    async fn subscribe(&self, plan: Kind, sender: impl Into<Sender<Message>>) -> Result<(), Error> {
        if !self.is_connected() {
            return Err(PulseError::NotConnected.into())
        }

        let sender = sender.into();

        let subscribe_callback = Box::new({
            let context = Arc::downgrade(&self.context);
            let peakers = Arc::downgrade(&self.peakers);

            move |facility, op, i| {
                subscribe_callback(&sender, &context, &peakers, facility, op, i)
            }
        });

        let mut mask = InterestMaskSet::NULL;

        if plan.contains(Kind::Software) {
            mask |= InterestMaskSet::SINK_INPUT;
        }

        if plan.contains(Kind::Hardware) {
            mask |= InterestMaskSet::SINK;
            mask |= InterestMaskSet::SERVER;
        }

        let guard = self.lock().await;
        let mut context = guard.borrow_mut();

        context.set_subscribe_callback(Some(subscribe_callback));
        context.subscribe(mask, |_| ());

        Ok(())
    }

    async fn set_volume(&self, ids: impl IntoIterator<Item = u32>, kind: Kind, levels: VolumeLevels) {
        if !self.is_connected() {
            return
        }

        let context = self.lock().await;
        let mut introspect = context.introspect();

        let volume: ChannelVolumes = levels.into();

        for id in ids.into_iter() {
            match kind {
                k if k.contains(Kind::Out | Kind::Software) => {
                    introspect.set_sink_input_volume(id, &volume, None);
                },
                k if k.contains(Kind::Out | Kind::Hardware) => {
                    introspect.set_sink_volume_by_index(id, &volume, None);
                },
                _ => {}
            };
        }
    }

    async fn set_mute(&self, ids: impl IntoIterator<Item = u32>, kind: Kind, flag: bool) {
        if !self.is_connected() {
            return
        }

        let context = self.lock().await;
        let mut introspect = context.introspect();

        for id in ids.into_iter() {
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

    async fn set_output_by_name(&self, name: &str, port: Option<&str>) {
        if !self.is_connected() || name.is_empty() {
            return
        }

        let context = self.lock().await;

        if let Some(port) = port {
            let mut introspect = context.introspect();

            introspect.set_sink_port_by_name(name, port, None);
        }

        context.borrow_mut().set_default_sink(name, |_| {});
    }
}

fn add_sink_input(info: ListResult<&SinkInputInfo>, context: &WeakContext, sender: &Sender<Message>, peakers: &WeakPeakers)
{
    let Some(context) = context.upgrade() else { return };
    let Some(peakers) = peakers.upgrade() else { return };

    if let ListResult::Item(info) = info {
        if !info.has_volume { return }

        let client: Box<OutputClient> = Box::new(info.into());
        let id = client.id;

        let msg: Message = MessageClient::New(client).into();
        sender.emit(msg);

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
            let msg: Message = MessageClient::Peak(i, peak).into();

            if peak != 0.0 { sender.emit(msg); }
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
            rate: std::env::var("PULSE_PEAK_RATE").ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(DEFAULT_PEAK_RATE)
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

fn handle_server_change(sender: &Sender<Message>, context: &WeakContext) {
    let Some(introspect) = try_introspect(context) else { return };

    let context = context.clone();
    let sender = sender.clone();

    introspect.get_server_info(move |info| {
        let Some(introspect) = try_introspect(&context) else { return };
        let Some(name) = &info.default_sink_name else { return };

        let sender = sender.clone();

        let output_name = name.to_string();

        introspect.get_sink_info_by_name(name, move |info| {
            let ListResult::Item(info) = info else {
                return
            };

            let Some(port_name) = info.active_port.as_ref().and_then(|p| p.name.as_ref()) else {
                return
            };

            let output = Output {
                name: output_name.to_string(),
                port: port_name.to_string(),
                master: true,
            };

            let msg: Message = MessageOutput::Master(output).into();
            sender.emit(msg);

            let client = Box::new(info.into());
            let msg: Message = MessageClient::Changed(client).into();
            sender.emit(msg);
        });
    });
}

fn handle_sink_change(sender: &Sender<Message>, context: &WeakContext) {
    let Some(introspect) = try_introspect(context) else { return };

    introspect.get_sink_info_by_index(0, {
        let sender = sender.clone();

        move |info| if let ListResult::Item(info) = info {
            let client = Box::new(info.into());
            let msg: Message = MessageClient::Changed(client).into();

            sender.emit(msg);
        }
    });
}

fn handle_sink_input_change(sender: &Sender<Message>, context: &WeakContext, peakers: &WeakPeakers, op: Operation, i: u32) {
    let Some(introspect) = try_introspect(context) else { return };

    match op {
        Operation::New => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();
                let context = context.clone();
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

            let msg: Message = MessageClient::Removed(i).into();
            sender.emit(msg);
        },
        Operation::Changed => {
            introspect.get_sink_input_info(i, {
                let sender = sender.clone();

                move |info| {
                    if let ListResult::Item(info) = info {
                        let client = Box::new(info.into());
                        let msg: Message = MessageClient::Changed(client).into();

                        sender.emit(msg);
                    };
                }
            });
        },
    }
}

fn subscribe_callback(sender: &Sender<Message>, context: &WeakContext, peakers: &WeakPeakers, facility: Option<Facility>, op: Option<Operation>, i: u32) {
    let Some(op) = op else { return };

    match facility {
        Some(Facility::SinkInput) => {
            handle_sink_input_change(sender, context, peakers, op, i);
        },
        Some(Facility::Sink) => {
            handle_sink_change(sender, context);
        }
        Some(Facility::Server) => {
            handle_server_change(sender, context);
        },
        _ => {},
    }

}

fn state_callback(context: &WeakContext, state: &Weak<AtomicU8>, sender: &Sender<Message>) {
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

fn try_introspect(context: &WeakContext) -> Option<Introspector> {
    let context = context.upgrade()?;

    let guard = context.lock();
    let context = guard.borrow_mut();

    Some(context.introspect())
}

#[derive(Deref)]
struct ContextRef<'a> {
    #[deref]
    context: MutexGuard<'a, RefCell<Context>>,
    lock: &'a watch::Sender<Lock>,
    thread: MutexGuard<'a, Thread>,
}

impl<'a> ContextRef<'a> {
    fn introspect(&self) -> Introspector {
        let context = self.borrow();
        context.introspect()
    }
}

impl Drop for ContextRef<'_> {
    #[inline]
    fn drop(&mut self) {
        self.lock.send_replace(Lock::Unlocked);
        self.thread.unpark();
    }
}

#[derive(From)]
struct Duration(std::time::Duration);

impl From<&Duration> for Option<libpulse_binding::time::MicroSeconds> {
    #[inline]
    fn from(d: &Duration) -> Self {
        match d.0.is_zero() {
            false => Some(libpulse_binding::time::MicroSeconds(d.0.as_micros() as u64)),
            true => None,
        }
    }
}

impl From<VolumeLevels> for ChannelVolumes {
    fn from(levels: VolumeLevels) -> Self {
        let mut cv = ChannelVolumes::default();
        cv.set_len(levels.len() as u8);
        cv.get_mut().copy_from_slice(unsafe {
            std::mem::transmute::<&[u32], &[libpulse_binding::volume::Volume]>(&levels)
        });
        cv
    }
}

impl Volume {
    fn pulse_linear(&self) -> f64 {
        use libpulse_binding::volume::{Volume, VolumeLinear};

        let max = *self.levels.iter().max().unwrap_or(&0);
        VolumeLinear::from(Volume(max)).0
    }

    fn set_pulse_linear(&mut self, v: f64) {
        use libpulse_binding::volume::{Volume, VolumeLinear};

        let v = Volume::from(VolumeLinear(v)).0;
        let max = *self.levels.iter().max().unwrap();

        if max > Volume::MUTED.0 {
            self.levels.iter_mut()
                .for_each(|i| *i = ((*i as u64 * v as u64 / max as u64) as u32).clamp(Volume::MUTED.0, Volume::MAX.0));
        }
        else { self.levels.fill(v); }
    }
}

impl <'a> From<&SinkInputInfo<'a>> for OutputClient {
    fn from(sink_input: &SinkInputInfo<'a>) -> Self {
        let name = sink_input.proplist.get_str("application.name").unwrap_or_default();
        let description = sink_input.name.as_ref().map(Cow::to_string).unwrap_or_default();
        let icon = sink_input.proplist.get_str("application.icon_name");
        let process = sink_input.proplist.get_str("application.process.id")
            .and_then(|b| b.parse::<u32>().ok());

        // This would be the correct approach, but things get weird after 255%
        // static VOLUME_MAX: OnceLock<f64> = OnceLock::new();
        // let max = *VOLUME_MAX.get_or_init(|| VolumeLinear::from(libpulse_binding::volume::Volume::ui_max()).0);

        let volume = Volume {
            levels: {
                let levels: &[u32] = unsafe {
                    use libpulse_binding::volume::Volume;

                    std::mem::transmute::<&[Volume], &[u32]>(sink_input.volume.get())
                };

                VolumeLevels(SmallVec::from_slice(&levels[..sink_input.volume.len() as usize]))
            },
            percent: &Volume::pulse_linear,
            set_percent: &Volume::set_pulse_linear,
        };

        OutputClient {
            id: sink_input.index,
            process,
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

impl <'a> From<&SinkInfo<'a>> for OutputClient {
    fn from(sink: &SinkInfo<'a>) -> Self {
        let description = sink.active_port
            .as_ref()
            .and_then(|port| port.description.to_owned())
            .unwrap_or_default()
            .to_string();

        let volume = Volume {
            levels: {
                let levels: &[u32] = unsafe {
                    use libpulse_binding::volume::Volume;

                    std::mem::transmute::<&[Volume], &[u32]>(sink.volume.get())
                };
                VolumeLevels(SmallVec::from_slice(&levels[..sink.volume.len() as usize]))
            },
            percent: &|v: &Volume| {
                use libpulse_binding::volume::Volume;

                *v.levels.iter().max().unwrap() as f64 / Volume::NORMAL.0 as f64
            },
            set_percent: &|v: &mut Volume, p: f64| {
                v.levels.fill((libpulse_binding::volume::Volume::NORMAL.0 as f64 * p) as u32);
            },
        };

        OutputClient {
            id: 0,
            process: None,
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
