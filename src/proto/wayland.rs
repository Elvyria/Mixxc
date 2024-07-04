use relm4::component::AsyncComponent;

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::{anchor::Anchor, app::App, label, warnln};

impl App where Self: AsyncComponent {
    pub fn init_wayland(window: &<Self as AsyncComponent>::Root, anchors: Anchor, margins: &[i32], focusable: bool) {
        if !gtk4_layer_shell::is_supported() {
            warnln!("You're using Wayland, but your compositor doesn't support {} protocol.", label::LAYER_SHELL_PROTOCOL);
            return
        }

        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace("volume-mixer");

        if focusable {
            window.set_keyboard_mode(KeyboardMode::OnDemand);
        }

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

