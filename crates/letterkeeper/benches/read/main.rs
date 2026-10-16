// SPDX-License-Identifier: GPL-3.0-only
#[path = "../support/entry.rs"]
mod entry;
fn main() -> Result<(), String> {
    let options = entry::Options::parse(&std::env::args_os().skip(1).collect::<Vec<_>>())?;
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/read/measure.py");
    entry::measure(&script, &options, None)
}
