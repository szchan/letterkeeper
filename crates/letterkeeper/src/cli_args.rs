// SPDX-License-Identifier: GPL-3.0-only
use clap::{Parser, Subcommand};
use std::{net::SocketAddr, path::PathBuf};
#[derive(Parser)]
#[command(
    version,
    about = "LetterKeeper privacy prototype (controlled protocols; real accounts not yet verified)"
)]
pub(super) struct Cli {
    #[arg(long, global = true, default_value = ".local/letterkeeper")]
    pub(super) state_dir: PathBuf,
    #[arg(long, global = true)]
    pub(super) config_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(super) data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(super) account: Option<String>,
    #[command(subcommand)]
    pub(super) command: Action,
}
#[derive(Subcommand)]
pub(super) enum Action {
    /// Inspect local environment without creating storage or checking account authentication.
    Doctor,
    /// Parse an explicitly selected EML through the isolated worker.
    Read { file: PathBuf },
    /// Browse an account or explicitly selected synthetic EML files.
    Tui {
        #[arg(long)]
        mail: Vec<PathBuf>,
        #[arg(long)]
        read_only: bool,
        /// Explicit loopback SMTP for synthetic offline identities only.
        #[arg(long)]
        smtp_test: Option<SocketAddr>,
    },
    /// Serve the startup-selected account through local MCP stdio.
    Mcp {
        #[arg(long)]
        mail: Vec<PathBuf>,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        smtp_test: Option<SocketAddr>,
    },
    /// Manage account configuration and secure credentials.
    Account {
        #[command(subcommand)]
        command: AccountAction,
    },
    /// Scoped account folders, reading, replies, attachments and synchronization.
    Mail {
        #[command(subcommand)]
        command: MailAction,
    },
    /// Local drafts and explicit human authorization (use --account for remote accounts).
    Draft {
        #[command(subcommand)]
        command: DraftAction,
    },
    /// Submit an approved snapshot; --smtp-test is only for the offline synthetic demo.
    Send {
        id: i64,
        #[arg(long)]
        smtp_test: Option<SocketAddr>,
    },
    /// Query SMTP acceptance/failure/uncertainty; acceptance is not final delivery.
    Status { id: i64 },
}
#[derive(Subcommand)]
pub(super) enum AccountAction {
    /// Interactively review and create configuration; authentication is separate.
    Add,
    Import {
        file: PathBuf,
    },
    List,
    Password {
        id: String,
    },
    ClientSecret {
        id: String,
    },
    Login {
        id: String,
    },
    Logout {
        id: String,
    },
    Revoke {
        id: String,
    },
}
#[derive(Subcommand)]
pub(super) enum DraftAction {
    Create {
        file: PathBuf,
    },
    Show {
        id: i64,
    },
    Approve {
        id: i64,
    },
    Update {
        id: i64,
        #[arg(long)]
        revision: i64,
        file: PathBuf,
    },
    AddAttachment {
        id: i64,
        file: PathBuf,
        #[arg(long, default_value = "application/octet-stream")]
        content_type: String,
    },
}
#[derive(Subcommand)]
pub(super) enum MailAction {
    Folders,
    List {
        #[arg(long)]
        folder: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Search {
        query: String,
        #[arg(long)]
        folder: Option<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Read {
        id: String,
    },
    Reply {
        id: String,
        #[arg(long,default_value="reply",value_parser=["reply","reply_all","forward"])]
        mode: String,
        #[arg(long)]
        body_file: PathBuf,
        #[arg(long)]
        to: Vec<String>,
    },
    SaveAttachment {
        id: String,
        part: usize,
        directory: PathBuf,
    },
    MarkSeen {
        id: String,
    },
    SyncDraft {
        id: i64,
    },
    SyncSent {
        id: i64,
    },
    SyncStatus {
        id: i64,
    },
    SentEvidence {
        id: i64,
    },
    RecoverSend {
        id: i64,
    },
}
