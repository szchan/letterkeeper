// SPDX-License-Identifier: GPL-3.0-only
use clap::{Parser, Subcommand};
use letterkeeper::{
    app::{AppHost, draft_view, now, store_at, submission_view, validate_demo},
    sandbox::Sandbox,
};
use letterkeeper_adapters::Host;
use letterkeeper_content::{Limits, SafeDocument};
use letterkeeper_core::DraftInput;
use std::{
    fs::File,
    io::{IsTerminal, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Parser)]
#[command(
    version,
    about = "LetterKeeper offline privacy prototype (synthetic mail only)"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".local/letterkeeper")]
    state_dir: PathBuf,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Parse an EML file through the isolated worker and print safe JSON.
    Read { file: PathBuf },
    /// Read explicitly selected EML files in the terminal (maximum 50).
    Tui {
        #[arg(long, required = true)]
        mail: Vec<PathBuf>,
    },
    /// Serve the fixed local-demo account through MCP stdio.
    Mcp {
        #[arg(long)]
        mail: Vec<PathBuf>,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        smtp_test: Option<SocketAddr>,
    },
    /// Create, inspect and explicitly authorize local prototype drafts.
    Draft {
        #[command(subcommand)]
        command: DraftAction,
    },
    /// Submit an approved synthetic draft to a loopback test SMTP server.
    Send {
        id: i64,
        #[arg(long)]
        smtp_test: SocketAddr,
    },
    /// Query a persisted submission. Accepted means SMTP accepted, not delivered.
    Status { id: i64 },
}
#[derive(Subcommand)]
enum DraftAction {
    /// Load a bounded JSON DraftInput from a user-selected local file.
    Create {
        file: PathBuf,
    },
    Show {
        id: i64,
    },
    /// Review the exact snapshot and type its full digest in a real terminal.
    Approve {
        id: i64,
    },
}
fn bounded_file(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "input_file_unavailable")?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_file_unavailable")?;
    if bytes.len() > max {
        return Err("input_too_large".into());
    }
    Ok(bytes)
}
fn documents(paths: &[PathBuf]) -> Result<Vec<SafeDocument>, String> {
    if paths.len() > 50 {
        return Err("too_many_messages".into());
    }
    let sandbox = Sandbox::current()?;
    paths
        .iter()
        .map(|path| sandbox.parse(&bounded_file(path, Limits::default().max_raw_bytes)?))
        .collect()
}
fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, value).map_err(|_| "output_failed")?;
    writeln!(out).map_err(|_| "output_failed".into())
}
fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Action::Read { file } => print_json(&documents(&[file])?.remove(0)),
        Action::Tui { mail } => {
            let host = Arc::new(AppHost::open(
                &cli.state_dir,
                documents(&mail)?,
                true,
                None,
            )?);
            letterkeeper_adapters::run_tui(host).map_err(|_| "terminal_unavailable".into())
        }
        Action::Mcp {
            mail,
            read_only,
            smtp_test,
        } => {
            let host = Arc::new(AppHost::open(
                &cli.state_dir,
                documents(&mail)?,
                read_only,
                smtp_test,
            )?);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "runtime_unavailable")?;
            runtime
                .block_on(letterkeeper_adapters::run_mcp(host))
                .map_err(|_| "mcp_session_failed".into())
        }
        Action::Draft { command } => match command {
            DraftAction::Create { file } => {
                let input: DraftInput =
                    serde_json::from_slice(&bounded_file(&file, 48 * 1024 * 1024)?)
                        .map_err(|_| "invalid_draft_json")?;
                validate_demo(&input)?;
                let draft = store_at(&cli.state_dir, false)?
                    .create_draft(input)
                    .map_err(|e| e.to_string())?;
                print_json(&draft_view(&draft))
            }
            DraftAction::Show { id } => print_json(&draft_view(
                &store_at(&cli.state_dir, true)?
                    .get_draft(id)
                    .map_err(|e| e.to_string())?,
            )),
            DraftAction::Approve { id } => {
                if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                    return Err("interactive_terminal_required".into());
                }
                let mut store = store_at(&cli.state_dir, false)?;
                let draft = store.get_draft(id).map_err(|e| e.to_string())?;
                validate_demo(&draft.input)?;
                print_json(&draft_view(&draft))?;
                println!(
                    "Review every recipient, body and attachment. Approval expires in 5 minutes.\nType the full digest to approve this exact snapshot:"
                );
                std::io::stdout().flush().map_err(|_| "output_failed")?;
                let mut answer = String::new();
                std::io::stdin().take(256).read_line_limited(&mut answer)?;
                if answer.trim() != draft.digest {
                    return Err("approval_cancelled".into());
                }
                store
                    .approve(id, &draft.digest, now()? + 300)
                    .map_err(|e| e.to_string())?;
                println!("approved");
                Ok(())
            }
        },
        Action::Send { id, smtp_test } => {
            let host = AppHost::open(&cli.state_dir, vec![], false, Some(smtp_test))?;
            print_json(
                &host
                    .send_draft(&id.to_string())
                    .map_err(|e| e.to_string())?,
            )
        }
        Action::Status { id } => print_json(&submission_view(
            &store_at(&cli.state_dir, true)?
                .get_submission(id)
                .map_err(|e| e.to_string())?,
        )),
    }
}
// Bounded single-line reading without consuming unrelated terminal input.
trait ReadLineLimited: Read {
    fn read_line_limited(&mut self, out: &mut String) -> Result<(), String> {
        let mut bytes = Vec::new();
        let mut one = [0];
        while self.read(&mut one).map_err(|_| "terminal_read_failed")? != 0 {
            if one[0] == b'\n' {
                break;
            }
            bytes.push(one[0]);
        }
        *out = String::from_utf8(bytes).map_err(|_| "invalid_confirmation")?;
        Ok(())
    }
}
impl<T: Read> ReadLineLimited for T {}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("__worker" | "__probe")
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
