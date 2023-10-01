use relm4::{gtk, Component};

use gtk::prelude::{Cast, NativeExt, WidgetExt};
use gdk_x11::X11Surface;

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{PropMode, AtomEnum, ClientMessageEvent, CLIENT_MESSAGE_EVENT, EventMask};
use x11rb::x11_utils::Serialize;

use anyhow::Error;

use crate::app::App;

impl App where Self: Component {
    pub fn realize_x11(window: &<Self as Component>::Root) {
        let Ok(xsurface) = window.surface().downcast::<X11Surface>() else {
            return
        };

        let xid = xsurface.xid() as u32;

        let atoms = match realize(xid) {
            Ok(atoms) => atoms,
            Err(e) => { eprintln!("{}", e); return }
        };

        window.connect_map(move |_| {
            if let Err(e) = map(atoms, xid) {
                eprintln!("{}", e);
            }
        });
    }
}

// Specification:
// https://specifications.freedesktop.org/wm-spec/1.5/ar01s04.html
fn realize(xid: u32) -> Result<AtomCollection, Error> {
    use x11rb::wrapper::ConnectionExt;

    let (conn, _) = x11rb::connect(None)?;

    let cookie = AtomCollection::new(&conn)?;
    let atoms = cookie.reply()?;

    conn.change_property32(PropMode::REPLACE,
                           xid,
                           atoms._NET_WM_WINDOW_TYPE,
                           AtomEnum::ATOM,
                           &[atoms._NET_WM_WINDOW_TYPE_UTILITY])?.check()?;

    conn.change_property32(PropMode::REPLACE,
                           xid,
                           atoms._NET_WM_ALLOWED_ACTIONS,
                           AtomEnum::ATOM,
                           &[atoms._NET_WM_ACTION_CLOSE, atoms._NET_WM_ACTION_ABOVE])?.check()?;

    conn.change_property32(PropMode::REPLACE,
                           xid,
                           atoms._NET_WM_BYPASS_COMPOSITOR,
                           AtomEnum::CARDINAL,
                           &[2])?.check()?;

    Ok(atoms)
}

fn map(atoms: AtomCollection, xid: u32) -> Result<(), Error> {
    let (conn, _) = x11rb::connect(None)?;

    add_wm_state(&conn, xid, atoms, atoms._NET_WM_STATE_ABOVE, atoms._NET_WM_STATE_STICKY)?;
    add_wm_state(&conn, xid, atoms, atoms._NET_WM_STATE_SKIP_TASKBAR, atoms._NET_WM_STATE_SKIP_PAGER)?;

    Ok(())
}

fn send_message(conn: &impl Connection, xid: u32, event: ClientMessageEvent) -> Result<(), ReplyError> {
    use x11rb::protocol::xproto::ConnectionExt;

    conn.send_event(false, xid, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::STRUCTURE_NOTIFY, event.serialize())?.check()
}

fn add_wm_state(conn: &impl Connection, xid: u32, atoms: AtomCollection, s1: u32, s2: u32) -> Result<(), ReplyError> {
    const _NET_WM_STATE_ADD: u32 = 1;
    const _NET_WM_STATE_APP: u32 = 1;

    let message = ClientMessageEvent {
        response_type: CLIENT_MESSAGE_EVENT,
        format:        32,
        sequence:      0,
        window:        xid,
        type_:         atoms._NET_WM_STATE,
        data:          [_NET_WM_STATE_ADD, s1, s2, _NET_WM_STATE_APP, 0].into(),
    };

    send_message(conn, xid, message)
}

x11rb::atom_manager! {
    pub AtomCollection: AtomCollectionCookie {
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_STICKY,

        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_UTILITY,

        _NET_WM_BYPASS_COMPOSITOR,

        _NET_WM_ALLOWED_ACTIONS,
        _NET_WM_ACTION_CLOSE,
        _NET_WM_ACTION_ABOVE,
    }
}
