use color_print::cstr;

pub const ERROR: &str = cstr!("<s><r>Error</></>");

#[allow(dead_code)]
pub const WARNING: &str = cstr!("<r>Warning</>");

#[cfg(not(feature = "Wayland"))]
pub const WAYLAND: &str = cstr!("<g>Wayland</>");

#[cfg(not(feature = "X11"))]
pub const X11: &str = cstr!("<g>X11</>");

#[cfg(not(feature = "Sass"))]
pub const SASS: &str = cstr!("<g>Sass</>");

#[cfg(feature = "Wayland")]
pub const LAYER_SHELL_PROTOCOL: &str = cstr!("<g>zwlr_layer_shell_v1</>");
