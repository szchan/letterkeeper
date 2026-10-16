// SPDX-License-Identifier: GPL-3.0-only
//! Common argument and process boundary for the two stable Cargo bench targets.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Options {
    pub binary: PathBuf,
    pub output: PathBuf,
}
impl Options {
    pub fn parse(arguments: &[OsString]) -> Result<Self, String> {
        let mut binary = None;
        let mut output = None;
        let mut values = arguments.iter().filter(|arg| *arg != "--bench");
        while let Some(name) = values.next() {
            let destination = match name.to_str() {
                Some("--binary") => &mut binary,
                Some("--output") => &mut output,
                _ => return Err("expected --binary FILE --output JSON".into()),
            };
            let value = values.next().ok_or("missing benchmark option value")?;
            if destination.is_some()
                || value.is_empty()
                || value.to_string_lossy().starts_with("--")
            {
                return Err("duplicate or missing benchmark option".into());
            }
            *destination = Some(PathBuf::from(value));
        }
        Ok(Self {
            binary: binary.ok_or("--binary is required")?,
            output: output.ok_or("--output is required")?,
        })
    }
}

pub fn measure(script: &Path, options: &Options, driver: Option<&Path>) -> Result<(), String> {
    let binary = options.binary.canonicalize().map_err(|e| e.to_string())?;
    if !binary.is_file() {
        return Err("benchmark binary must be a regular file".into());
    }
    let output = std::path::absolute(&options.output).map_err(|e| e.to_string())?;
    if output == binary || output.canonicalize().ok().as_ref() == Some(&binary) {
        return Err("benchmark output cannot overwrite the application".into());
    }
    std::fs::create_dir_all(output.parent().ok_or("output has no parent")?)
        .map_err(|e| e.to_string())?;
    let mut command = Command::new("python3");
    command
        .arg(script)
        .arg("--binary")
        .arg(binary)
        .arg("--output")
        .arg(&output);
    if let Some(driver) = driver {
        command.arg("--driver").arg(driver);
    }
    let status = command.status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("benchmark measurement failed: {status}"));
    }
    let json = std::fs::read(&output).map_err(|e| e.to_string())?;
    serde_json::from_slice::<serde_json::Value>(&json).map_err(|e| e.to_string())?;
    Ok(())
}
