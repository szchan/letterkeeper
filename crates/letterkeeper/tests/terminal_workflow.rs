// SPDX-License-Identifier: GPL-3.0-only
//! Real PTY smoke for the human workspace. Child uses synthetic local state only.
use letterkeeper::app::AppHost;
use letterkeeper::remote::RemoteHost;
use letterkeeper_accounts::{AccountConfig, AccountError, CredentialStore, Secret, SecretKind};
use letterkeeper_adapters::{
    DraftInput as AgentDraft, Host, HostError, MessageSummary, run_tui_workspace,
    workspace::HumanWorkspace,
};
use letterkeeper_content::SafeDocument;
use letterkeeper_core::DraftInput;
use serde_json::Value;
#[path = "support/performance_server.rs"]
mod performance_server;
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream, UdpSocket},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::process::CommandExt,
    },
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn workspace_pty_child() {
    let Some(path) = std::env::var_os("LETTERKEEPER_WORKSPACE_TEST_STATE") else {
        return;
    };
    let host = Arc::new(AppHost::open(std::path::Path::new(&path), vec![], false, None).unwrap());
    let browser: Arc<dyn Host> = host.clone();
    let human: Arc<dyn HumanWorkspace> = host;
    run_tui_workspace(browser, human).unwrap();
}

#[test]
fn workspace_smtp_pty_child() {
    let Some(path) = std::env::var_os("LETTERKEEPER_WORKSPACE_SMTP_STATE") else {
        return;
    };
    let endpoint: SocketAddr = std::env::var("LETTERKEEPER_WORKSPACE_SMTP_ENDPOINT")
        .unwrap()
        .parse()
        .unwrap();
    let host = Arc::new(
        AppHost::open(std::path::Path::new(&path), vec![], false, Some(endpoint)).unwrap(),
    );
    let browser: Arc<dyn Host> = host.clone();
    let human: Arc<dyn HumanWorkspace> = host;
    run_tui_workspace(browser, human).unwrap();
}

fn smtp_once(accept: bool) -> (SocketAddr, std::sync::mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(b"220 synthetic.example.test ESMTP\r\n")
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if line.starts_with("EHLO ") {
                stream
                    .write_all(b"250-synthetic.example.test\r\n250 8BITMIME\r\n")
                    .unwrap();
            } else if line.starts_with("MAIL FROM:") || line.starts_with("RCPT TO:") {
                stream.write_all(b"250 accepted\r\n").unwrap();
            } else if line.trim() == "DATA" {
                stream.write_all(b"354 send data\r\n").unwrap();
                let mut message = String::new();
                loop {
                    let mut part = String::new();
                    if reader.read_line(&mut part).unwrap_or(0) == 0 {
                        return;
                    }
                    if part == ".\r\n" {
                        break;
                    }
                    message.push_str(&part);
                }
                tx.send(message).unwrap();
                if accept {
                    stream.write_all(b"250 accepted\r\n").unwrap();
                } else {
                    break;
                }
            } else if line.trim() == "QUIT" {
                let _ = stream.write_all(b"221 bye\r\n");
                break;
            } else {
                stream.write_all(b"250 ok\r\n").unwrap();
            }
        }
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            if listener.accept().is_ok() {
                let _ = tx.send("unexpected retry".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (address, rx)
}

struct BenchmarkHost {
    messages: Vec<MessageSummary>,
    body_bytes: usize,
}
impl Host for BenchmarkHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self
            .messages
            .iter()
            .skip(offset)
            .take(limit.min(50))
            .cloned()
            .collect())
    }
    fn search_messages(
        &self,
        _: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self
            .messages
            .iter()
            .filter(|m| m.subject.contains(query))
            .skip(offset)
            .take(limit.min(50))
            .cloned()
            .collect())
    }
    fn read_message(&self, id: &str) -> Result<SafeDocument, HostError> {
        if !self.messages.iter().any(|m| m.id == id) {
            return Err(HostError("missing".into()));
        }
        Ok(SafeDocument {
            subject: id.into(),
            from: "sender@example.test".into(),
            text: "中".repeat(self.body_bytes / 3),
            links: vec![],
            attachments: vec![],
            truncated: false,
            warnings: vec![],
        })
    }
    fn create_draft(&self, _: AgentDraft) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn get_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_status(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
}
fn benchmark_draft(account: &str, from: &str, body_bytes: usize) -> DraftInput {
    DraftInput {
        account: account.into(),
        from: from.into(),
        to: vec!["recipient@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: "Benchmark editor".into(),
        body: "x".repeat(body_bytes.min(1024 * 1024 - 20)),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
    }
}
#[test]
fn workspace_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    assert!(count == 1_000 || count == 10_000);
    let body_bytes = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    assert!(body_bytes == 32 * 1024 || body_bytes == 2 * 1024 * 1024);
    let messages = (0..count)
        .map(|i| MessageSummary {
            id: format!("mail-{i}"),
            subject: format!("Synthetic subject {i:05}"),
            from: "sender@example.test".into(),
        })
        .collect();
    let host: Arc<dyn Host> = Arc::new(BenchmarkHost {
        messages,
        body_bytes,
    });
    let dir = tempfile::tempdir().unwrap();
    let human = Arc::new(AppHost::open(&dir.path().join("private"), vec![], false, None).unwrap());
    human
        .create(benchmark_draft(
            "local-demo",
            "sender@example.test",
            body_bytes,
        ))
        .unwrap();
    let human: Arc<dyn HumanWorkspace> = human;
    run_tui_workspace(host, human).unwrap();
}

struct SyntheticVault;
impl CredentialStore for SyntheticVault {
    fn get(&self, _: &str, _: SecretKind) -> Result<Option<Secret>, AccountError> {
        Ok(Some(Secret::new("synthetic-secret".into())))
    }
    fn set(&self, _: &str, _: SecretKind, _: &Secret) -> Result<(), AccountError> {
        Ok(())
    }
    fn delete(&self, _: &str, _: SecretKind) -> Result<(), AccountError> {
        Ok(())
    }
}
#[test]
fn workspace_tls_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_TLS_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    let raw_bytes: usize = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    let server = performance_server::Server::start(count, raw_bytes);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &server.root_pem).unwrap();
    let config: AccountConfig = serde_json::from_value(serde_json::json!({
        "id":"benchmark", "email":"person@example.test", "username":"person@example.test",
        "imap":{"host":"localhost","port":server.port},
        "smtp":{"host":"localhost","port":9465},
        "allowed_folders":["INBOX","Drafts"],
        "permissions":{"read":true,"draft":true,"send":false},
        "drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed",
        "authentication":{"type":"password"},"extra_ca_pem":ca,
    }))
    .unwrap();
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: env!("CARGO_BIN_EXE_letterkeeper").into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host = Arc::new(
        RemoteHost::open(
            config,
            &dir.path().join("state"),
            Arc::new(SyntheticVault),
            sandbox,
        )
        .unwrap(),
    );
    host.create(benchmark_draft(
        "benchmark",
        "person@example.test",
        raw_bytes,
    ))
    .unwrap();
    let browser: Arc<dyn Host> = host.clone();
    let human: Arc<dyn HumanWorkspace> = host;
    run_tui_workspace(browser, human).unwrap();
}

#[test]
fn workspace_tls_mcp_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_MCP_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    let raw_bytes: usize = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    let server = performance_server::Server::start(count, raw_bytes);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &server.root_pem).unwrap();
    let config: AccountConfig = serde_json::from_value(serde_json::json!({
        "id":"benchmark", "email":"person@example.test", "username":"person@example.test",
        "imap":{"host":"localhost","port":server.port},
        "smtp":{"host":"localhost","port":9465}, "allowed_folders":["INBOX"],
        "permissions":{"read":true,"draft":false,"send":false},
        "drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed",
        "authentication":{"type":"password"},"extra_ca_pem":ca,
    }))
    .unwrap();
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: env!("CARGO_BIN_EXE_letterkeeper").into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host: Arc<dyn Host> = Arc::new(
        RemoteHost::open(
            config,
            &dir.path().join("state"),
            Arc::new(SyntheticVault),
            sandbox,
        )
        .unwrap(),
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(letterkeeper_adapters::run_mcp(host)).unwrap();
}

#[test]
fn remote_human_workspace_shares_controlled_tls_mail_and_drafts() {
    let server = performance_server::Server::start(1_000, 32 * 1024);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &server.root_pem).unwrap();
    let config: AccountConfig = serde_json::from_value(serde_json::json!({
        "id":"benchmark", "email":"person@example.test", "username":"person@example.test",
        "imap":{"host":"localhost","port":server.port},
        "smtp":{"host":"localhost","port":9465}, "allowed_folders":["INBOX","Drafts"],
        "permissions":{"read":true,"draft":true,"send":false},
        "drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed",
        "authentication":{"type":"password"},"extra_ca_pem":ca,
    }))
    .unwrap();
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: env!("CARGO_BIN_EXE_letterkeeper").into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host = RemoteHost::open(
        config,
        &dir.path().join("state"),
        Arc::new(SyntheticVault),
        sandbox,
    )
    .unwrap();
    let messages = host.list_messages(0, 1).unwrap();
    assert_eq!(messages.len(), 1);
    let read = host.read_message(&messages[0].id).unwrap();
    assert!(read.subject.contains("Synthetic subject"));
    let draft = host
        .create(DraftInput {
            account: "benchmark".into(),
            from: "person@example.test".into(),
            to: vec!["recipient@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "中文回复".into(),
            body: "受控协议草稿".into(),
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        })
        .unwrap();
    assert_eq!(host.list_drafts(0, 50).unwrap()[0].id, draft.id);
    let review = host.review(draft.id).unwrap();
    assert_eq!(review.draft.digest, draft.digest);
    assert!(host.send(draft.id).is_err(), "send permission is disabled");
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn recv(master: &mut File, marker: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let marker = compact(marker.as_bytes());
    let mut buf = [0u8; 8192];
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        match master.read(&mut buf) {
            Ok(n) if n > 0 => output.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            _ => (),
        }
        if plain(&output).windows(marker.len()).any(|w| w == marker) {
            return output;
        }
        assert!(
            output.len() < 512 * 1024,
            "terminal generated unbounded output"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "PTY missing marker {marker:?}; captured {} bytes",
        output.len()
    );
}
fn compact(data: &[u8]) -> Vec<u8> {
    data.iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect()
}
fn plain(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && data.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < data.len() {
                let b = data[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    compact(&result)
}
fn size(slave: &File, width: u16, height: u16) {
    let winsize = libc::winsize {
        ws_row: height,
        ws_col: width,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: valid slave PTY descriptor and initialized winsize.
    assert_eq!(
        unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &winsize) },
        0
    );
}

#[test]
fn workspace_four_sizes_editing_q_and_terminal_restore() {
    for (width, height) in [(120, 40), (80, 24), (40, 12), (20, 6)] {
        let dir = tempfile::tempdir().unwrap();
        let (mut master_fd, mut slave_fd) = (-1, -1);
        let winsize = libc::winsize {
            ws_row: height,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: pointers refer to valid storage and winsize is initialized.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master_fd,
                    &mut slave_fd,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &winsize,
                )
            },
            0
        );
        // SAFETY: openpty gave us two owned FDs, each transferred once.
        let (mut master, slave) =
            unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
        let before = unsafe {
            let mut value = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), value.as_mut_ptr()), 0);
            value.assume_init()
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "workspace_pty_child", "--nocapture"])
            .env(
                "LETTERKEEPER_WORKSPACE_TEST_STATE",
                dir.path().join("private"),
            )
            .env("TERM", "xterm-256color")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap());
        // SAFETY: after fork, only async-signal-safe session and ioctl calls are used.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = Child(command.spawn().unwrap());
        // SAFETY: owned live PTY master descriptor.
        assert_ne!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            -1
        );
        recv(&mut master, "LetterKeeper");
        master.write_all(b"c").unwrap();
        recv(&mut master, "新建");
        master.write_all(b"q").unwrap();
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "q must be text inside the editor"
        );
        size(
            &slave,
            width.saturating_sub(1).max(10),
            height.saturating_sub(1).max(4),
        );
        // SAFETY: child owns this PTY session and SIGWINCH only requests redraw.
        assert_eq!(
            unsafe { libc::kill(child.0.id() as i32, libc::SIGWINCH) },
            0
        );
        master.write_all(b"\x1b").unwrap();
        recv(&mut master, "未保存");
        master.write_all(b"d").unwrap();
        master.write_all(b"q").unwrap();
        let start = Instant::now();
        while child.0.try_wait().unwrap().is_none() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "terminal did not exit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let after = unsafe {
            let mut value = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), value.as_mut_ptr()), 0);
            value.assume_init()
        };
        assert_eq!(
            (
                after.c_iflag,
                after.c_oflag,
                after.c_cflag,
                after.c_lflag,
                after.c_cc
            ),
            (
                before.c_iflag,
                before.c_oflag,
                before.c_cflag,
                before.c_lflag,
                before.c_cc
            )
        );
    }
}

#[test]
fn pty_approval_requires_full_digest_and_f9_sends_once() {
    for accept in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (endpoint, captures) = smtp_once(accept);
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let dns = UdpSocket::bind("127.0.0.1:0").unwrap();
        let connection = TcpStream::connect(http.local_addr().unwrap()).unwrap();
        let _ = http.accept().unwrap();
        drop(connection);
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(b"positive-control", dns.local_addr().unwrap())
            .unwrap();
        assert!(dns.recv_from(&mut [0; 512]).unwrap().0 > 0);
        http.set_nonblocking(true).unwrap();
        dns.set_nonblocking(true).unwrap();
        let state = dir.path().join("private");
        let host = AppHost::open(&state, vec![], false, Some(endpoint)).unwrap();
        let draft = host
            .create(DraftInput {
                account: "local-demo".into(),
                from: "sender@example.test".into(),
                to: vec!["recipient@example.test".into()],
                cc: vec!["copy@example.test".into()],
                bcc: vec!["hidden@example.test".into()],
                subject: "Synthetic SMTP".into(),
                body: format!("Chinese body 你好 q\n<img src=\"http://{}/pixel\">\n<link rel=\"dns-prefetch\" href=\"//tracker.example.test\">\n{}\nTAIL_SENTINEL", http.local_addr().unwrap(), "中文长行 body\n".repeat(2_000)),
                attachments: vec![],
                in_reply_to: None,
                references: vec![],
            })
            .unwrap();
        let (mut master_fd, mut slave_fd) = (-1, -1);
        let winsize = libc::winsize {
            ws_row: 40,
            ws_col: 120,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: initialized arguments and two writable FD slots.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master_fd,
                    &mut slave_fd,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &winsize,
                )
            },
            0
        );
        // SAFETY: openpty transferred both fresh descriptors exactly once.
        let (mut master, slave) =
            unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "workspace_smtp_pty_child", "--nocapture"])
            .env("LETTERKEEPER_WORKSPACE_SMTP_STATE", &state)
            .env("LETTERKEEPER_WORKSPACE_SMTP_ENDPOINT", endpoint.to_string())
            .env("TERM", "xterm-256color")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap());
        // SAFETY: child closure uses only async-signal-safe session/ioctl calls.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = Child(command.spawn().unwrap());
        // SAFETY: live owned PTY master.
        assert_ne!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            -1
        );
        recv(&mut master, "LetterKeeper");
        master.write_all(b"d").unwrap();
        recv(&mut master, "Synthetic SMTP");
        master.write_all(b"\r").unwrap();
        recv(&mut master, "Ctrl+S");
        master.write_all(b"\x1b[17~").unwrap(); // F6 review
        recv(&mut master, "完整审阅");
        master.write_all(b"\x1b[F").unwrap(); // End reaches the body tail beyond 16 KiB.
        recv(&mut master, "TAIL_SENTINEL");
        master.write_all(b"\x1b[19~").unwrap(); // F8 approval dialog
        recv(&mut master, "输入完整摘要");
        assert!(
            captures.try_recv().is_err(),
            "review/approval dialog must not send"
        );
        master.write_all(draft.digest.as_bytes()).unwrap();
        master.write_all(b"\r").unwrap();
        recv(&mut master, "批准成功");
        assert!(captures.try_recv().is_err(), "approval must not send");
        master.write_all(b"\x1b[20~").unwrap(); // F9 explicit send
        recv(&mut master, if accept { "Accepted" } else { "Uncertain" });
        let message = captures.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(message.contains("recipient@example.test"));
        assert!(!message.to_lowercase().contains("bcc:"));
        assert!(
            captures.recv_timeout(Duration::from_millis(500)).is_err(),
            "no automatic retry"
        );
        master.write_all(b"q").unwrap();
        assert!(child.0.wait().unwrap().success());
        let submissions = host.list_submissions(0, 50).unwrap();
        assert_eq!(submissions.len(), 1);
        assert_eq!(
            format!("{:?}", submissions[0].state),
            if accept { "Accepted" } else { "Uncertain" }
        );
        let queried = host.submission_status(submissions[0].id).unwrap();
        assert_eq!(queried.state, submissions[0].state);
        assert_eq!(
            http.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            dns.recv_from(&mut [0; 512]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
