// SPDX-License-Identifier: GPL-3.0-only
#[path = "../support/entry.rs"]
mod entry;
mod workloads;
fn main() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "--child") {
        if arguments.len() != 2 {
            return Err("benchmark child requires one workload".into());
        }
        return workloads::child(arguments[1].to_str().ok_or("invalid workload name")?);
    }
    let options = entry::Options::parse(&arguments)?;
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/workspace/measure.py");
    entry::measure(
        &script,
        &options,
        Some(&std::env::current_exe().map_err(|e| e.to_string())?),
    )
}
