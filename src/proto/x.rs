use std::rc::Rc;

use relm4::component::AsyncComponent;

use gtk::prelude::{Cast, NativeExt, WidgetExt, SurfaceExt};

use gdk_x11::{X11Surface, X11Display};

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xinerama::get_screen_size;
use x11rb::protocol::xproto::{PropMode, AtomEnum, ClientMessageEvent, CLIENT_MESSAGE_EVENT, EventMask, ConnectionExt, ConfigureWindowAux};
use x11rb::x11_utils::Serialize;

use crate::anchor::Anchor;
use crate::app::App;

impl App where Self: AsyncComponent {
    pub fn realize_x11(window: &<Self as AsyncComponent>::Root, anchors: Anchor, margins: Vec<i32>) {
        let surface = window.surface().unwrap();

        let Ok(xsurface) = surface.downcast::<X11Surface>() else {
            return
        };

        let Ok(xdisplay) = window.display().downcast::<X11Display>() else {
            return
        };

        let (conn, _) = x11rb::connect(None).expect("connecting to X11");
        let atoms = AtomCollection::new(&conn).unwrap().reply().expect("baking atomic cookie");

        let conn = Rc::new(conn);

        let xid = xsurface.xid() as u32;

        set_wm_properties(conn.as_ref(), atoms, xid).expect("setting WM properties");

        let screen_num = xdisplay.screen().screen_number() as u32;
        let screen = get_screen_size(conn.as_ref(), xid, screen_num).unwrap().reply().expect("collecting screen info");

        window.connect_map({
            let conn = conn.clone();

            move |_| { // Place window off-screen while initializing
                let config = ConfigureWindowAux::new().x(screen.width as i32).y(screen.height as i32);
                conn.configure_window(xid, &config).unwrap().check().expect("hiding window offscreen");

                add_wm_states(conn.as_ref(), atoms, xid).expect("updating _NET_WM_STATE");
            }
        });

        xsurface.connect_layout({
            move |_, width, height| {
                let (x, y) = anchors.position(&margins,
                                              (screen.width, screen.height),
                                              (width as u32, height as u32));

                let config = ConfigureWindowAux::new().x(x).y(y);
                conn.configure_window(xid, &config).unwrap().check().expect("moving window with `xcb_configure_window`");
            }
        });
    }
}

// Specification:
// https://specifications.freedesktop.org/wm-spec/1.5/ar01s04.html
fn set_wm_properties(conn: &impl Connection, atoms: AtomCollection, xid: u32) -> Result<(), ReplyError> {
    use x11rb::wrapper::ConnectionExt;

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

    Ok(())
}

fn add_wm_states(conn: &impl Connection, atoms: AtomCollection, xid: u32) -> Result<(), ReplyError> {
    add_wm_state(conn, xid, atoms, atoms._NET_WM_STATE_ABOVE, atoms._NET_WM_STATE_STICKY)?;
    add_wm_state(conn, xid, atoms, atoms._NET_WM_STATE_SKIP_TASKBAR, atoms._NET_WM_STATE_SKIP_PAGER)?;

    Ok(())
}

fn send_message(conn: &impl Connection, xid: u32, event: ClientMessageEvent) -> Result<(), ReplyError> {
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
