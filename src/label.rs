use color_print::cstr;

pub static ERROR: &str = cstr!("<s><r>Error</></>");

#[allow(dead_code)]
pub static WARNING: &str = cstr!("<r>Warning</>");

#[cfg(not(feature = "Wayland"))]
pub static WAYLAND: &str = cstr!("<g>Wayland</>");

#[cfg(not(feature = "X11"))]
pub static X11: &str = cstr!("<g>X11</>");

#[cfg(not(feature = "Sass"))]
pub static SASS: &str = cstr!("<g>Sass</>");
