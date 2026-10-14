// SPDX-License-Identifier: GPL-3.0-only
use clap::{Parser, Subcommand};
use letterkeeper::{
    app::{AppHost, draft_view, now, store_at, submission_view, validate_demo},
    files::{export_attachment, import_attachment, read_regular},
    remote::{RemoteHost, store_for_account, validate_identity},
    sandbox::Sandbox,
};
use letterkeeper_accounts::{
    AccountConfig, AccountRepository, Authentication, CredentialStore, GmailOAuth, Secret,
    SecretKind, SystemCredentialStore,
};
use letterkeeper_adapters::Host;
use letterkeeper_content::{Limits, SafeDocument};
use letterkeeper_core::{DraftInput, Store};
use std::{
    io::{IsTerminal, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[derive(Parser)]
#[command(
    version,
    about = "LetterKeeper privacy prototype (controlled protocols; real accounts not yet verified)"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".local/letterkeeper")]
    state_dir: PathBuf,
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    account: Option<String>,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
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
enum AccountAction {
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
enum DraftAction {
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
enum MailAction {
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
fn repository(config: Option<&Path>, data: Option<&Path>) -> Result<AccountRepository, String> {
    let (default_config, default_data) =
        AccountRepository::default_paths().map_err(|e| e.to_string())?;
    let absolute = |p: &Path| -> Result<PathBuf, String> {
        if p.is_absolute() {
            Ok(p.into())
        } else {
            Ok(std::env::current_dir()
                .map_err(|_| "directory_unavailable")?
                .join(p))
        }
    };
    AccountRepository::new(
        absolute(config.unwrap_or(&default_config))?,
        absolute(data.unwrap_or(&default_data))?,
    )
    .map_err(|e| e.to_string())
}
fn documents(paths: &[PathBuf]) -> Result<Vec<SafeDocument>, String> {
    if paths.is_empty() || paths.len() > 50 {
        return Err("select_one_to_fifty_messages".into());
    }
    let sandbox = Sandbox::current()?;
    paths
        .iter()
        .map(|p| sandbox.parse(&read_regular(p, Limits::default().max_raw_bytes)?))
        .collect()
}
fn print_json(value: &impl serde::Serialize) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, value).map_err(|_| "output_failed")?;
    writeln!(out).map_err(|_| "output_failed".into())
}
fn terminal_required() -> Result<(), String> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        Ok(())
    } else {
        Err("interactive_terminal_required".into())
    }
}
fn remote(account: Option<&(AccountConfig, PathBuf)>) -> Result<Arc<RemoteHost>, String> {
    let (config, path) = account.ok_or("account_required_use_global_account_option")?;
    Ok(Arc::new(RemoteHost::open(
        config.clone(),
        path,
        Arc::new(SystemCredentialStore::new()),
        Sandbox::current()?,
    )?))
}
fn scoped_store(
    account: Option<&(AccountConfig, PathBuf)>,
    offline: &Path,
) -> Result<Store, String> {
    match account {
        Some((cfg, path)) => store_for_account(cfg, path),
        None => store_at(offline, false),
    }
}
fn validate_input(
    account: Option<&(AccountConfig, PathBuf)>,
    input: &DraftInput,
) -> Result<(), String> {
    match account {
        Some((cfg, _)) => validate_identity(cfg, input),
        None => validate_demo(input),
    }
}
fn account_action(repo: AccountRepository, action: AccountAction) -> Result<(), String> {
    let id = match &action {
        AccountAction::Add => return Err("interactive_terminal_required".into()),
        AccountAction::Import { file } => {
            let cfg: AccountConfig = serde_json::from_slice(&read_regular(file, 65536)?)
                .map_err(|_| "invalid_account_json")?;
            repo.save(&cfg).map_err(|e| e.to_string())?;
            return print_json(&serde_json::json!({"id":cfg.id,"saved":true}));
        }
        AccountAction::List => return print_json(&repo.list().map_err(|e| e.to_string())?),
        AccountAction::Password { id }
        | AccountAction::ClientSecret { id }
        | AccountAction::Login { id }
        | AccountAction::Logout { id }
        | AccountAction::Revoke { id } => id,
    };
    let cfg = repo.load(id).map_err(|e| e.to_string())?;
    let state = repo.data_dir().join(&cfg.id);
    let vault = SystemCredentialStore::new();
    // Ensure private runtime storage, without accessing any credential yet.
    store_for_account(&cfg, &state)?;
    match action {
        AccountAction::Add => Err("interactive_terminal_required".into()),
        AccountAction::Password { .. } | AccountAction::ClientSecret { .. } => {
            terminal_required()?;
            let kind = if matches!(action, AccountAction::Password { .. }) {
                if cfg.authentication != Authentication::Password {
                    return Err("account_uses_oauth".into());
                }
                SecretKind::Password
            } else {
                if !matches!(
                    cfg.authentication,
                    Authentication::GmailOAuth {
                        client_secret_ref: true,
                        ..
                    }
                ) {
                    return Err("client_secret_reference_not_configured".into());
                }
                SecretKind::ClientSecret
            };
            let _lock = letterkeeper::remote::credential_lock(&vault, &state, &cfg.id)?;
            let secret = Secret::new(
                rpassword::prompt_password("Secret (hidden input): ")
                    .map_err(|_| "terminal_read_failed")?,
            );
            if secret.expose().is_empty()
                || secret.expose().len() > 4096
                || secret.expose().chars().any(char::is_control)
            {
                return Err("invalid_secret_input".into());
            }
            vault
                .set(&cfg.id, kind, &secret)
                .map_err(|e| e.to_string())?;
            print_json(&serde_json::json!({"id":cfg.id,"credential_saved":true}))
        }
        AccountAction::Login { .. } => {
            terminal_required()?;
            let _lock = letterkeeper::remote::credential_lock(&vault, &state, &cfg.id)?;
            let oauth = GmailOAuth::new(&cfg, &vault).map_err(|e| e.to_string())?;
            let mut login = oauth.begin().map_err(|e| e.to_string())?;
            println!(
                "Open this URL in your system browser to authorize this account:\n{}",
                login.authorization_url()
            );
            std::io::stdout().flush().map_err(|_| "output_failed")?;
            oauth
                .complete(&mut login, Duration::from_secs(300))
                .map_err(|e| e.to_string())?;
            print_json(&serde_json::json!({"id":cfg.id,"authorized":true}))
        }
        AccountAction::Logout { .. } | AccountAction::Revoke { .. } => {
            let _lock = letterkeeper::remote::credential_lock(&vault, &state, &cfg.id)?;
            match cfg.authentication {
                Authentication::Password => {
                    if matches!(action, AccountAction::Revoke { .. }) {
                        return Err("password_account_has_no_oauth_revocation".into());
                    }
                    vault
                        .delete(&cfg.id, SecretKind::Password)
                        .map_err(|e| e.to_string())?;
                }
                Authentication::GmailOAuth { .. } => {
                    let oauth = GmailOAuth::new(&cfg, &vault).map_err(|e| e.to_string())?;
                    if matches!(action, AccountAction::Revoke { .. }) {
                        oauth.revoke()
                    } else {
                        oauth.logout()
                    }
                    .map_err(|e| e.to_string())?;
                }
            }
            print_json(&serde_json::json!({"id":cfg.id,"logged_out":true}))
        }
        _ => unreachable!(),
    }
}
fn run(cli: Cli) -> Result<(), String> {
    if matches!(
        &cli.command,
        Action::Account {
            command: AccountAction::Add
        }
    ) {
        terminal_required()?;
        let cfg = letterkeeper::onboarding::collect(
            &mut std::io::stdin().lock(),
            &mut std::io::stdout().lock(),
        )?;
        if let Some(cfg) = cfg {
            repository(cli.config_dir.as_deref(), cli.data_dir.as_deref())?
                .save(&cfg)
                .map_err(|e| e.to_string())?;
            return print_json(
                &serde_json::json!({"configuration_saved": true, "authentication": "unverified"}),
            );
        }
        return print_json(&serde_json::json!({"configuration_saved": false, "cancelled":true}));
    }
    let account = cli
        .account
        .as_deref()
        .map(|id| {
            let repo = repository(cli.config_dir.as_deref(), cli.data_dir.as_deref())?;
            let cfg = repo.load(id).map_err(|e| e.to_string())?;
            let state = repo.data_dir().join(id);
            Ok::<_, String>((cfg, state))
        })
        .transpose()?;
    match cli.command {
        Action::Read { file } => print_json(&documents(&[file])?.remove(0)),
        Action::Account { command } => account_action(
            repository(cli.config_dir.as_deref(), cli.data_dir.as_deref())?,
            command,
        ),
        Action::Tui {
            mail,
            read_only,
            smtp_test,
        } => {
            terminal_required()?;
            let (host, workspace): (
                Arc<dyn Host>,
                Arc<dyn letterkeeper_adapters::workspace::HumanWorkspace>,
            ) = if let Some((cfg, state)) = &account {
                if !mail.is_empty() || smtp_test.is_some() {
                    return Err("cannot_mix_account_and_test_transport".into());
                }
                let mut cfg = cfg.clone();
                if read_only {
                    cfg.permissions.draft = false;
                    cfg.permissions.send = false;
                }
                let host = Arc::new(RemoteHost::open(
                    cfg,
                    state,
                    Arc::new(SystemCredentialStore::new()),
                    Sandbox::current()?,
                )?);
                (host.clone(), host)
            } else {
                let host = Arc::new(AppHost::open(
                    &cli.state_dir,
                    if mail.is_empty() {
                        vec![]
                    } else {
                        documents(&mail)?
                    },
                    read_only,
                    smtp_test,
                )?);
                (host.clone(), host)
            };
            letterkeeper_adapters::run_tui_workspace(host, workspace)
                .map_err(|_| "terminal_unavailable".into())
        }
        Action::Mcp {
            mail,
            read_only,
            smtp_test,
        } => {
            let host: Arc<dyn Host> = if let Some((cfg, state)) = &account {
                if !mail.is_empty() || smtp_test.is_some() {
                    return Err("cannot_mix_account_and_test_transport".into());
                }
                let mut cfg = cfg.clone();
                if read_only {
                    cfg.permissions.draft = false;
                    cfg.permissions.send = false;
                }
                Arc::new(RemoteHost::open(
                    cfg,
                    state,
                    Arc::new(SystemCredentialStore::new()),
                    Sandbox::current()?,
                )?)
            } else {
                Arc::new(AppHost::open(
                    &cli.state_dir,
                    if mail.is_empty() {
                        vec![]
                    } else {
                        documents(&mail)?
                    },
                    read_only,
                    smtp_test,
                )?)
            };
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "runtime_unavailable")?
                .block_on(letterkeeper_adapters::run_mcp(host))
                .map_err(|_| "mcp_session_failed".into())
        }
        Action::Mail { command } => {
            let host = remote(account.as_ref())?;
            match command {
                MailAction::Folders => print_json(&host.list_folders().map_err(|e| e.to_string())?),
                MailAction::List {
                    folder,
                    offset,
                    limit,
                } => print_json(
                    &host
                        .list_folder_messages(folder.as_deref(), offset, limit)
                        .map_err(|e| e.to_string())?,
                ),
                MailAction::Search {
                    query,
                    folder,
                    offset,
                    limit,
                } => print_json(
                    &host
                        .search_messages(folder.as_deref(), &query, offset, limit)
                        .map_err(|e| e.to_string())?,
                ),
                MailAction::Read { id } => {
                    print_json(&host.read_message(&id).map_err(|e| e.to_string())?)
                }
                MailAction::Reply {
                    id,
                    mode,
                    body_file,
                    to,
                } => {
                    let body = String::from_utf8(read_regular(&body_file, 1024 * 1024)?)
                        .map_err(|_| "body_requires_utf8")?;
                    print_json(
                        &host
                            .reply_draft(&id, &mode, &body, &to)
                            .map_err(|e| e.to_string())?,
                    )
                }
                MailAction::SaveAttachment {
                    id,
                    part,
                    directory,
                } => {
                    let item = host.attachment(&id, part).map_err(|e| e.to_string())?;
                    export_attachment(&directory, &item.name, &item.bytes)?;
                    print_json(
                        &serde_json::json!({"saved":true,"name":item.name,"bytes":item.bytes.len()}),
                    )
                }
                MailAction::MarkSeen { id } => {
                    host.mark_seen(&id).map_err(|e| e.to_string())?;
                    print_json(&serde_json::json!({"marked_seen":true}))
                }
                MailAction::SyncDraft { id } => print_json(
                    &host
                        .sync_draft(&id.to_string())
                        .map_err(|e| e.to_string())?,
                ),
                MailAction::SyncSent { id } => {
                    print_json(&host.sync_sent(&id.to_string()).map_err(|e| e.to_string())?)
                }
                MailAction::SyncStatus { id } => print_json(
                    &host
                        .sync_status(&id.to_string())
                        .map_err(|e| e.to_string())?,
                ),
                MailAction::SentEvidence { id } => print_json(
                    &host
                        .sent_evidence(&id.to_string())
                        .map_err(|e| e.to_string())?,
                ),
                MailAction::RecoverSend { id } => {
                    print_json(&host.recover_sender(id).map_err(|e| e.to_string())?)
                }
            }
        }
        Action::Draft { command } => match command {
            DraftAction::Create { file } => {
                let input: DraftInput =
                    serde_json::from_slice(&read_regular(&file, 48 * 1024 * 1024)?)
                        .map_err(|_| "invalid_draft_json")?;
                validate_input(account.as_ref(), &input)?;
                print_json(&draft_view(
                    &scoped_store(account.as_ref(), &cli.state_dir)?
                        .create_draft(input)
                        .map_err(|e| e.to_string())?,
                ))
            }
            DraftAction::Show { id } => print_json(&draft_view(
                &scoped_store(account.as_ref(), &cli.state_dir)?
                    .get_draft(id)
                    .map_err(|e| e.to_string())?,
            )),
            DraftAction::Update { id, revision, file } => {
                let input: DraftInput =
                    serde_json::from_slice(&read_regular(&file, 48 * 1024 * 1024)?)
                        .map_err(|_| "invalid_draft_json")?;
                validate_input(account.as_ref(), &input)?;
                print_json(&draft_view(
                    &scoped_store(account.as_ref(), &cli.state_dir)?
                        .update_draft(id, revision, input)
                        .map_err(|e| e.to_string())?,
                ))
            }
            DraftAction::AddAttachment {
                id,
                file,
                content_type,
            } => {
                let item = import_attachment(&file, &content_type)?;
                let mut s = scoped_store(account.as_ref(), &cli.state_dir)?;
                let d = s.get_draft(id).map_err(|e| e.to_string())?;
                let mut input = d.input;
                input.attachments.push(item);
                validate_input(account.as_ref(), &input)?;
                print_json(&draft_view(
                    &s.update_draft(id, d.revision, input)
                        .map_err(|e| e.to_string())?,
                ))
            }
            DraftAction::Approve { id } => {
                terminal_required()?;
                let mut store = scoped_store(account.as_ref(), &cli.state_dir)?;
                let draft = store.get_draft(id).map_err(|e| e.to_string())?;
                validate_input(account.as_ref(), &draft.input)?;
                print_json(&draft_view(&draft))?;
                println!(
                    "Review every recipient, body, thread header and attachment. Approval expires in 5 minutes.\nType the full digest to approve this exact snapshot:"
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
            if account.is_some() {
                if smtp_test.is_some() {
                    return Err("test_transport_not_allowed_for_account".into());
                }
                print_json(
                    &remote(account.as_ref())?
                        .send_draft(&id.to_string())
                        .map_err(|e| e.to_string())?,
                )
            } else {
                let endpoint = smtp_test.ok_or("offline_demo_requires_smtp_test")?;
                print_json(
                    &AppHost::open(&cli.state_dir, vec![], false, Some(endpoint))?
                        .send_draft(&id.to_string())
                        .map_err(|e| e.to_string())?,
                )
            }
        }
        Action::Status { id } => print_json(&submission_view(
            &scoped_store(account.as_ref(), &cli.state_dir)?
                .get_submission(id)
                .map_err(|e| e.to_string())?,
        )),
    }
}
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
