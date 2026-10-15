// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]
use letterkeeper_protocol::{Credentials, Endpoint, TlsOptions};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
#[derive(Clone, Copy)]
pub enum Scenario {
    Imap,
    Paged { count: u32, mode: PageMode },
    Mailbox,
    Recovery,
    Utf8Search,
    BadCharset,
    MaxBody,
    HugeHeader,
    Stall,
    AppendFinalReject,
    SmtpFinalReject,
    HugeLiteral,
    AuthReject,
    AppendDisconnect,
    AppendReject,
    Smtp,
    SmtpReject,
    SmtpDisconnect,
}
#[derive(Clone, Copy)]
pub enum PageMode {
    All,
    Sparse,
    Empty,
    Rejected,
    Duplicate,
    Zero,
    TooMany,
    Expunge,
    ChangedExists,
    ChangedValidity,
    WrongTag,
    MissingSearch,
    HugeLine,
    MissingFetch,
    DuplicateFetch,
    UnexpectedFetch,
    FetchRejected,
    FetchWrongTag,
    FetchDrift,
    CrossWindowOverlap,
    ReverseOrder,
    MultipleSearch,
}
pub fn page_uid(sequence: u32) -> u32 {
    1_000_000_000 + sequence * 3
}
pub struct Server {
    pub endpoint: Endpoint,
    pub tls: TlsOptions,
    pub transcript: Arc<Mutex<Vec<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}
impl Server {
    /// Persistent synthetic mailbox; serves sequential independent TLS sessions until finish().
    pub fn start_shared() -> Self {
        Self::with_name(Scenario::Mailbox, "localhost")
    }
    pub fn start(scenario: Scenario) -> Self {
        Self::with_name(scenario, "localhost")
    }
    pub fn with_name(scenario: Scenario, name: &str) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec![name.into()]).unwrap();
        let pem = cert.cert.pem().into_bytes();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.der().clone()], key.into())
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let transcript = Arc::new(Mutex::new(Vec::new()));
        let observed = transcript.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
                let shared = matches!(scenario, Scenario::Mailbox);
                let mut mailbox = Mailbox::default();
                loop {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let (tcp, _) = match tokio::time::timeout(
                        std::time::Duration::from_millis(if shared { 100 } else { 3000 }),
                        listener.accept(),
                    )
                    .await
                    {
                        Ok(Ok(pair)) => pair,
                        Err(_) if shared => continue,
                        _ => break,
                    };
                    let Ok(Ok(tls)) = tokio::time::timeout(
                        std::time::Duration::from_secs(3),
                        acceptor.accept(tcp),
                    )
                    .await
                    else {
                        if shared {
                            continue;
                        } else {
                            break;
                        }
                    };
                    let mut selected = "INBOX".to_string();
                    let mut io = BufReader::new(tls);
                    let smtp = matches!(
                        scenario,
                        Scenario::Smtp
                            | Scenario::SmtpReject
                            | Scenario::SmtpDisconnect
                            | Scenario::SmtpFinalReject
                    );
                    io.get_mut()
                        .write_all(if smtp {
                            b"220 synthetic.test ready\r\n"
                        } else {
                            b"* OK ready\r\n"
                        })
                        .await
                        .unwrap();
                    loop {
                        let mut line = String::new();
                        let Ok(Ok(n)) = tokio::time::timeout(
                            std::time::Duration::from_secs(3),
                            io.read_line(&mut line),
                        )
                        .await
                        else {
                            break;
                        };
                        if n == 0 {
                            break;
                        }
                        observed.lock().unwrap().push(line.clone());
                        let result = if smtp {
                            smtp_reply(&mut io, &line, scenario, &observed).await
                        } else {
                            if shared {
                                mailbox_reply(
                                    &mut io,
                                    &line,
                                    &observed,
                                    &mut mailbox,
                                    &mut selected,
                                )
                                .await
                            } else {
                                imap_reply(&mut io, &line, scenario, &observed).await
                            }
                        };
                        if !result {
                            break;
                        }
                    }
                    if !shared {
                        break;
                    }
                }
            });
        });
        Self {
            endpoint: Endpoint {
                host: "localhost".into(),
                port,
            },
            tls: TlsOptions {
                roots_pem: Some(pem),
            },
            transcript,
            thread: Some(thread),
            stop,
        }
    }
    pub fn credentials() -> Credentials {
        Credentials {
            username: "person@example.test".into(),
            secret: "synthetic-secret".into(),
            oauth: false,
        }
    }
    pub fn finish(mut self) -> Vec<String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
        self.transcript.lock().unwrap().clone()
    }
}
async fn imap_reply(
    io: &mut BufReader<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>,
    line: &str,
    scenario: Scenario,
    observed: &Arc<Mutex<Vec<String>>>,
) -> bool {
    let (tag, cmd) = line.trim_end().split_once(' ').unwrap();
    if let Scenario::Paged { count, mode } = scenario {
        return paged_reply(io, tag, cmd, count, mode, observed).await;
    }
    if matches!(scenario, Scenario::Stall) {
        tokio::time::sleep(std::time::Duration::from_millis(16000)).await;
        return false;
    }
    let reply = if cmd.starts_with("LOGIN") {
        format!("{tag} OK login\r\n")
    } else if cmd.starts_with("AUTHENTICATE") {
        io.get_mut().write_all(b"+ \r\n").await.unwrap();
        let mut response = String::new();
        io.read_line(&mut response).await.unwrap();
        observed.lock().unwrap().push(response);
        format!("{tag} NO denied synthetic-secret\r\n")
    } else if cmd.starts_with("LIST") {
        format!("* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n{tag} OK list\r\n")
    } else if cmd.starts_with("EXAMINE") || cmd.starts_with("SELECT") {
        format!("* 3 EXISTS\r\n* OK [UIDVALIDITY 77] stable\r\n{tag} OK [READ-ONLY] examine\r\n")
    } else if cmd.starts_with("UID SEARCH") {
        if matches!(scenario, Scenario::BadCharset) {
            format!("{tag} NO [BADCHARSET (US-ASCII)] rejected synthetic-secret private-search\r\n")
        } else if matches!(scenario, Scenario::Utf8Search) {
            if !cmd.is_ascii()
                || !(cmd.starts_with("UID SEARCH CHARSET UTF-8 ") && cmd.contains(" TEXT {"))
            {
                format!("{tag} NO [BADCHARSET (US-ASCII)] charset and literal required\r\n")
            } else {
                receive_search_literal(io, cmd, observed).await;
                format!("* SEARCH 11 12 13\r\n{tag} OK search\r\n")
            }
        } else {
            format!("* SEARCH 11 12 13\r\n{tag} OK search\r\n")
        }
    } else if cmd.starts_with("UID FETCH") {
        if matches!(scenario, Scenario::Recovery) && cmd.contains("HEADER.FIELDS") {
            let mut reply = String::new();
            for (uid, id) in [
                (11, "<sent@example.test>"),
                (12, "<other@example.test>"),
                (13, "<sent@example.test> extra"),
            ] {
                let header = format!("Message-ID: {id}\r\n\r\n");
                reply.push_str(&format!(
                    "* 1 FETCH (UID {uid} BODY[HEADER.FIELDS (MESSAGE-ID)] {{{}}}\r\n{header})\r\n",
                    header.len()
                ));
            }
            reply.push_str(&format!("{tag} OK fetch\r\n"));
            reply
        } else if matches!(scenario, Scenario::HugeHeader) {
            "* 1 FETCH (UID 11 BODY[HEADER] {65537}\r\n".to_string()
        } else if matches!(scenario, Scenario::MaxBody) {
            let body = "x".repeat(2 * 1024 * 1024);
            format!(
                "* 1 FETCH (UID 11 BODY[] {{{}}}\r\n{body})\r\n{tag} OK fetch\r\n",
                body.len()
            )
        } else if matches!(scenario, Scenario::HugeLiteral) {
            "* 1 FETCH (UID 11 BODY[] {999999999}\r\n".to_string()
        } else if cmd.contains("HEADER.FIELDS") {
            let header = "From: person@example.test\r\nSubject: synthetic\r\n\r\n";
            format!(
                "* 1 FETCH (UID 11 RFC822.SIZE 100 FLAGS () BODY[HEADER.FIELDS (FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES)] {{{}}}\r\n{header})\r\n{tag} OK fetch\r\n",
                header.len()
            )
        } else {
            let body = "From: person@example.test\r\n\r\nsynthetic body\r\n";
            format!(
                "* 1 FETCH (UID 11 BODY[] {{{}}}\r\n{body})\r\n{tag} OK fetch\r\n",
                body.len()
            )
        }
    } else if cmd.starts_with("APPEND") {
        if matches!(scenario, Scenario::AppendReject) {
            format!("{tag} NO rejected\r\n")
        } else {
            let size = cmd
                .rsplit_once('{')
                .unwrap()
                .1
                .trim_end_matches('}')
                .parse::<usize>()
                .unwrap();
            io.get_mut().write_all(b"+ ready\r\n").await.unwrap();
            let mut raw = vec![0; size + 2];
            io.read_exact(&mut raw).await.unwrap();
            observed
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&raw).into_owned());
            if matches!(scenario, Scenario::AppendDisconnect) {
                return false;
            }
            if matches!(scenario, Scenario::AppendFinalReject) {
                format!("{tag} NO rejected\r\n")
            } else {
                format!("{tag} OK append\r\n")
            }
        }
    } else if cmd.starts_with("UID STORE") {
        format!("{tag} OK store\r\n")
    } else if cmd.starts_with("LOGOUT") {
        let _ = io
            .get_mut()
            .write_all(format!("* BYE bye\r\n{tag} OK logout\r\n").as_bytes())
            .await;
        return false;
    } else {
        panic!("unexpected synthetic command {cmd}")
    };
    io.get_mut().write_all(reply.as_bytes()).await.is_ok()
}
async fn smtp_reply(
    io: &mut BufReader<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>,
    line: &str,
    scenario: Scenario,
    observed: &Arc<Mutex<Vec<String>>>,
) -> bool {
    let reply = if line.starts_with("EHLO") {
        "250-synthetic.test\r\n250-AUTH PLAIN XOAUTH2\r\n250 8BITMIME\r\n"
    } else if line.starts_with("AUTH") {
        "235 authenticated\r\n"
    } else if line.starts_with("MAIL") {
        "250 sender\r\n"
    } else if line.starts_with("RCPT") {
        if matches!(scenario, Scenario::SmtpReject) {
            "550 refused\r\n"
        } else {
            "250 recipient\r\n"
        }
    } else if line.starts_with("DATA") {
        io.get_mut().write_all(b"354 data\r\n").await.unwrap();
        let mut body = String::new();
        loop {
            let mut l = String::new();
            if io.read_line(&mut l).await.unwrap() == 0 {
                return false;
            }
            if l == ".\r\n" {
                break;
            }
            body.push_str(&l);
        }
        observed.lock().unwrap().push(body);
        if matches!(scenario, Scenario::SmtpDisconnect) {
            return false;
        }
        if matches!(scenario, Scenario::SmtpFinalReject) {
            "550 final refusal\r\n"
        } else {
            "250 accepted\r\n"
        }
    } else if line.starts_with("QUIT") {
        let _ = io.get_mut().write_all(b"221 bye\r\n").await;
        return false;
    } else {
        panic!("unexpected synthetic SMTP command")
    };
    io.get_mut().write_all(reply.as_bytes()).await.is_ok()
}

/// Synthetic mail only. It intentionally contains remote HTML resources so root
/// tests can verify that the separate content worker never requests them.
pub const MESSAGE:&[u8]=b"From: Author <author@example.test>\r\nTo: person@example.test\r\nCc: colleague@example.test\r\nReply-To: reply@example.test\r\nSubject: Synthetic thread\r\nDate: Tue, 13 Oct 2026 10:00:00 +0800\r\nMessage-ID: <inbox@example.test>\r\nReferences: <earlier@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=synthetic\r\n\r\n--synthetic\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>synthetic body</p><img src=\"https://resource.invalid/track\"><script>fetch('https://resource.invalid/script')</script>\r\n--synthetic\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"sample.txt\"\r\nContent-Transfer-Encoding: base64\r\n\r\nc3ludGhldGlj\r\n--synthetic--\r\n";
struct Mailbox {
    messages: Vec<(String, u32, Vec<u8>)>,
    next: u32,
}
impl Default for Mailbox {
    fn default() -> Self {
        Self {
            messages: vec![("INBOX".into(), 11, MESSAGE.to_vec())],
            next: 20,
        }
    }
}
fn field(raw: &[u8], name: &str) -> String {
    std::str::from_utf8(raw)
        .unwrap_or_default()
        .split("\r\n\r\n")
        .next()
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.trim().to_string())
        })
        .unwrap_or_default()
}
async fn mailbox_reply(
    io: &mut BufReader<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>,
    line: &str,
    observed: &Arc<Mutex<Vec<String>>>,
    mailbox: &mut Mailbox,
    selected: &mut String,
) -> bool {
    let (tag, cmd) = line.trim_end().split_once(' ').unwrap();
    let reply = if cmd.starts_with("LOGIN") {
        format!("{tag} OK login\r\n")
    } else if cmd.starts_with("LIST") {
        format!(
            "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n* LIST (\\Drafts) \"/\" \"Drafts\"\r\n* LIST (\\Sent) \"/\" \"Sent\"\r\n{tag} OK list\r\n"
        )
    } else if cmd.starts_with("EXAMINE") || cmd.starts_with("SELECT") {
        *selected = cmd.split_once(' ').unwrap().1.trim_matches('"').to_string();
        format!(
            "* {} EXISTS\r\n* OK [UIDVALIDITY 77] stable\r\n{tag} OK examine\r\n",
            mailbox
                .messages
                .iter()
                .filter(|(folder, _, _)| folder == selected)
                .count()
        )
    } else if cmd.starts_with("UID SEARCH") {
        if cmd.starts_with("UID SEARCH CHARSET UTF-8 ") && cmd.contains(" TEXT {") {
            receive_search_literal(io, cmd, observed).await;
        }
        let wanted = cmd
            .strip_prefix("UID SEARCH HEADER Message-ID ")
            .map(|s| s.trim_matches('"'));
        let ids = mailbox
            .messages
            .iter()
            .filter(|(folder, _, raw)| {
                folder == selected && wanted.is_none_or(|id| field(raw, "Message-ID") == id)
            })
            .map(|(_, uid, _)| uid.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "* SEARCH{space}{ids}\r\n{tag} OK search\r\n",
            space = if ids.is_empty() { "" } else { " " }
        )
    } else if cmd.starts_with("UID FETCH") {
        let set = cmd.split_whitespace().nth(2).unwrap();
        let mut reply = String::new();
        for (_, uid, raw) in mailbox.messages.iter().filter(|(folder, uid, _)| {
            folder == selected && set.split(',').any(|id| id == uid.to_string())
        }) {
            let (section, data) = if cmd.contains("HEADER.FIELDS") {
                (
                    "HEADER",
                    raw.split(|_| false).next().unwrap()
                        [..raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4]
                        .to_vec(),
                )
            } else {
                ("", raw.clone())
            };
            reply.push_str(&format!(
                "* 1 FETCH (UID {uid} RFC822.SIZE {} FLAGS () BODY[{section}] {{{}}}\r\n{})\r\n",
                raw.len(),
                data.len(),
                String::from_utf8_lossy(&data)
            ));
        }
        reply.push_str(&format!("{tag} OK fetch\r\n"));
        reply
    } else if cmd.starts_with("APPEND") {
        let folder = cmd
            .split_whitespace()
            .nth(1)
            .unwrap()
            .trim_matches('"')
            .to_string();
        let size = cmd
            .rsplit_once('{')
            .unwrap()
            .1
            .trim_end_matches('}')
            .parse::<usize>()
            .unwrap();
        io.get_mut().write_all(b"+ ready\r\n").await.unwrap();
        let mut raw = vec![0; size];
        if io.read_exact(&mut raw).await.is_err() {
            return false;
        }
        let mut end = [0; 2];
        if io.read_exact(&mut end).await.is_err() {
            return false;
        }
        observed
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(&raw).into_owned());
        mailbox.messages.push((folder, mailbox.next, raw));
        mailbox.next += 1;
        format!("{tag} OK append\r\n")
    } else if cmd.starts_with("UID STORE") {
        format!("{tag} OK store\r\n")
    } else if cmd.starts_with("LOGOUT") {
        let _ = io
            .get_mut()
            .write_all(format!("* BYE bye\r\n{tag} OK logout\r\n").as_bytes())
            .await;
        return false;
    } else {
        panic!("unexpected synthetic mailbox command {cmd}")
    };
    io.get_mut().write_all(reply.as_bytes()).await.is_ok()
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn receive_search_literal(
    io: &mut BufReader<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>,
    cmd: &str,
    observed: &Arc<Mutex<Vec<String>>>,
) {
    let count = cmd
        .rsplit_once('{')
        .unwrap()
        .1
        .strip_suffix('}')
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(count <= 1024);
    // A synchronizing literal must not arrive before the server continues.
    if let Ok(Ok(bytes)) =
        tokio::time::timeout(std::time::Duration::from_millis(30), io.fill_buf()).await
    {
        assert!(
            bytes.is_empty(),
            "search literal arrived before continuation"
        );
    }
    observed.lock().unwrap().push("search-continuation".into());
    io.get_mut()
        .write_all(b"+ ready for search\r\n")
        .await
        .unwrap();
    let mut raw = vec![0; count];
    io.read_exact(&mut raw).await.unwrap();
    let mut end = [0; 2];
    io.read_exact(&mut end).await.unwrap();
    assert_eq!(&end, b"\r\n");
    observed.lock().unwrap().push(format!(
        "search-literal:{}",
        String::from_utf8(raw).unwrap()
    ));
}

// Compact synthetic large mailbox: no real mail or account state is stored.
async fn paged_reply(
    io: &mut BufReader<tokio_rustls::server::TlsStream<tokio::net::TcpStream>>,
    tag: &str,
    cmd: &str,
    count: u32,
    mode: PageMode,
    observed: &Arc<Mutex<Vec<String>>>,
) -> bool {
    let reply = if cmd.starts_with("LOGIN") {
        format!("{tag} OK login\r\n")
    } else if cmd.starts_with("EXAMINE") {
        format!(
            "* {count} EXISTS\r\n* OK [UIDVALIDITY 77] stable\r\n{tag} OK [READ-ONLY] examine\r\n"
        )
    } else if cmd.starts_with("UID SEARCH") {
        if cmd.contains("CHARSET UTF-8") {
            receive_search_literal(io, cmd, observed).await;
        }
        let range = cmd.split_whitespace().find_map(|word| {
            let (start, end) = word.split_once(':')?;
            Some((start.parse::<u32>().ok()?, end.parse::<u32>().ok()?))
        });
        let (start, end) = range.unwrap_or((1, count));
        assert!(start >= 1 && start <= end && end <= count);
        if let Some((start, end)) = range {
            assert!(end - start < 4096, "sequence search must be bounded");
        }
        let mut ids: Vec<_> = (start..=end)
            .filter(|seq| match mode {
                PageMode::Empty => false,
                PageMode::Sparse => seq % 1000 == 0,
                _ => true,
            })
            .map(page_uid)
            .collect();
        match mode {
            PageMode::Rejected => {
                return io
                    .get_mut()
                    .write_all(format!("{tag} NO refused\r\n").as_bytes())
                    .await
                    .is_ok();
            }
            PageMode::Duplicate => ids = vec![page_uid(start), page_uid(start)],
            PageMode::Zero => ids = vec![0],
            PageMode::TooMany => ids = (1..=4097).map(page_uid).collect(),
            PageMode::CrossWindowOverlap if start > 1 => ids = vec![page_uid(1)],
            PageMode::ReverseOrder => ids.reverse(),
            _ => (),
        }
        let change = match mode {
            PageMode::Expunge => "* 1 EXPUNGE\r\n".to_string(),
            PageMode::ChangedExists => format!("* {} EXISTS\r\n", count + 1),
            PageMode::ChangedValidity => "* OK [UIDVALIDITY 78] changed\r\n".to_string(),
            _ => String::new(),
        };
        if matches!(mode, PageMode::HugeLine) {
            ids = (1..=20_000).map(page_uid).collect();
        }
        let mut search = if matches!(mode, PageMode::MissingSearch) {
            String::new()
        } else {
            format!(
                "* SEARCH{}\r\n",
                ids.iter().map(|id| format!(" {id}")).collect::<String>()
            )
        };
        if matches!(mode, PageMode::MultipleSearch) {
            search.push_str("* SEARCH\r\n");
        }
        let completed = if matches!(mode, PageMode::WrongTag) {
            "WRONG"
        } else {
            tag
        };
        format!("{change}{search}{completed} OK search\r\n")
    } else if cmd.starts_with("UID FETCH") {
        let mut ids: Vec<u32> = cmd
            .split_whitespace()
            .nth(2)
            .unwrap()
            .split(',')
            .map(|id| id.parse().unwrap())
            .collect();
        match mode {
            PageMode::MissingFetch => {
                ids.pop();
            }
            PageMode::DuplicateFetch => ids.push(ids[0]),
            PageMode::UnexpectedFetch => ids.push(42),
            _ => (),
        }
        let header = "From: synthetic@example.test\r\nSubject: bounded page\r\n\r\n";
        let mut reply = String::new();
        for uid in ids {
            reply.push_str(&format!("* 1 FETCH (UID {uid} RFC822.SIZE 100 FLAGS () BODY[HEADER] {{{}}}\r\n{header})\r\n", header.len()));
        }
        match mode {
            PageMode::FetchRejected => reply.push_str(&format!("{tag} NO refused\r\n")),
            PageMode::FetchWrongTag => {
                reply.push_str("WRONG OK fetch\r\n");
                // Old stream handling ignores a wrong tag; the later correct
                // completion lets the regression fail promptly by assertion.
                reply.push_str(&format!("{tag} OK fetch\r\n"));
            }
            PageMode::FetchDrift => reply.push_str(&format!("* 1 EXPUNGE\r\n{tag} OK fetch\r\n")),
            _ => reply.push_str(&format!("{tag} OK fetch\r\n")),
        }
        reply
    } else {
        panic!("unexpected synthetic paging command {cmd}")
    };
    io.get_mut().write_all(reply.as_bytes()).await.is_ok()
}
