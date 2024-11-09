use std::io::Write;
use std::fs::{self, File};

use anyhow::Result;

fn main() {
    compile_style().unwrap();
}

fn compile_style() -> Result<()> {
    use grass::*;

    let source      = "style/default.scss";
    let destination = "style/default.css";

    let source_mtime = fs::metadata(source)?.modified()?;

    if let Ok(destination_meta) = fs::metadata(destination) {
        if Some(source_mtime) == destination_meta.modified().ok() {
            return Ok(())
        }
    }

    let options = Options::default().style(OutputStyle::Expanded);
    let compiled = grass::from_path(source, &options)?;

    let mut f = File::create(destination)?;
    f.write_all(compiled.as_bytes())?;
    f.set_modified(source_mtime)?;

    Ok(())
}
