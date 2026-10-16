// SPDX-License-Identifier: GPL-3.0-only
mod cli_args;
mod commands;
use clap::Parser;
use cli_args::Cli;
use commands::run;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("__worker" | "__probe" | "__inspect" | "__attachment")
    ) {
        if let Err(code) = letterkeeper::sandbox::worker_entry(&args) {
            eprintln!("{code}");
            std::process::exit(1)
        }
        return;
    }
    if let Err(code) = run(Cli::parse()) {
        eprintln!("{code}");
        std::process::exit(1)
    }
}
