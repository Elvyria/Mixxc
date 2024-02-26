use relm4::Component;

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::{label, anchor::Anchor, app::App};

impl App where Self: Component {
    pub fn init_wayland(window: &<Self as Component>::Root, anchors: Anchor, margins: &[i32]) {
        if !gtk4_layer_shell::is_supported() {
            eprintln!("{}: You're using Wayland, but your compositor doesn't support {} protocol.", label::WARNING, label::LAYER_SHELL_PROTOCOL);
            return
        }

        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace("volume-mixer");
        window.set_keyboard_mode(KeyboardMode::OnDemand);

        for (i, anchor) in anchors.iter().enumerate() {
            let edge = anchor.try_into().unwrap();

            window.set_anchor(edge, true);
            window.set_margin(edge, *margins.get(i).unwrap_or(&0));
        }
    }
}

#[cfg(feature = "Wayland")]
impl TryFrom<Anchor> for Edge {
    type Error = ();

    fn try_from(anchor: Anchor) -> Result<Self, ()> {
        match anchor {
            Anchor::Top    => Ok(Edge::Top),
            Anchor::Left   => Ok(Edge::Left),
            Anchor::Bottom => Ok(Edge::Bottom),
            Anchor::Right  => Ok(Edge::Right),
            _              => Err(())
        }
    }
}

