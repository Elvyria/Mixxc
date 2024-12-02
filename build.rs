use std::io::Write;
use std::fs::{self, File};
use std::process::Command;

use anyhow::Result;

fn main() {
    compile_style().unwrap();
    git_hash().unwrap();
}

fn git() -> Command {
    Command::new("git")
}

fn git_hash() -> Result<()> {
    if !git().args(["describe", "--exact-match", "--tags", "HEAD"]).status().is_ok_and(|s| !s.success()) {
        return Ok(())
    }

    let output = git().args(["describe", "--tags", "HEAD"]).output()?;
    let output = String::from_utf8(output.stdout)?;

    println!("cargo:rustc-env=GIT_COMMIT={output}");

    Ok(())
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
