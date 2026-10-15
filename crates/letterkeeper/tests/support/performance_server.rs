// SPDX-License-Identifier: GPL-3.0-only
//! Sparse, synthetic, implicit-TLS IMAP server for release benchmarks.
#![allow(dead_code)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct Server {
    pub port: u16,
    pub root_pem: Vec<u8>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    pub fn start(count: usize, raw_bytes: usize) -> Self {
        assert!(count == 1_000 || count == 10_000);
        assert!(raw_bytes == 32 * 1024 || raw_bytes == 2 * 1024 * 1024);
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let root_pem = cert.cert.pem().into_bytes();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
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
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls));
                while !stopping.load(Ordering::Relaxed) {
                    let Ok(Ok((tcp, _))) = tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept()).await else { continue };
                    let Ok(Ok(tls)) = tokio::time::timeout(std::time::Duration::from_secs(3), acceptor.accept(tcp)).await else { continue };
                    let mut io = BufReader::new(tls);
                    if io.get_mut().write_all(b"* OK synthetic mailbox ready\r\n").await.is_err() { continue }
                    loop {
                        let mut line = String::new();
                        let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_secs(3), io.read_line(&mut line)).await else { break };
                        if n == 0 { break }
                        let Some((tag, command)) = line.trim_end().split_once(' ') else { break };
                        let reply = if command.starts_with("LOGIN") {
                            format!("{tag} OK login\r\n")
                        } else if command.starts_with("LIST") {
                            format!("* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n{tag} OK list\r\n")
                        } else if command.starts_with("EXAMINE") {
                            format!("* {count} EXISTS\r\n* OK [UIDVALIDITY 77] stable\r\n{tag} OK [READ-ONLY] examine\r\n")
                        } else if command.starts_with("UID SEARCH") {
                            if command.contains("CHARSET UTF-8 ") && command.contains(" TEXT {") {
                                if io.get_mut().write_all(b"+ ready\r\n").await.is_err() { break }
                                let mut literal = String::new();
                                if io.read_line(&mut literal).await.is_err() { break }
                            }
                            let (start, end) = command.split_whitespace().find_map(|word| {
                                let (start, end) = word.split_once(':')?;
                                Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                            }).unwrap_or((1, count));
                            let ids = (start..=end.min(count)).map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
                            format!("* SEARCH {ids}\r\n{tag} OK search\r\n")
                        } else if command.starts_with("UID FETCH") {
                            let set = command.split_whitespace().nth(2).unwrap_or("");
                            let mut out = String::new();
                            for uid in set.split(',').filter_map(|s| s.parse::<usize>().ok()).filter(|id| (1..=count).contains(id)) {
                                let head = format!("From: sender@example.test\r\nTo: person@example.test\r\nSubject: Synthetic subject {uid:05}\r\nMessage-ID: <synthetic-{uid}@example.test>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n");
                                if command.contains("HEADER.FIELDS") {
                                    out.push_str(&format!("* 1 FETCH (UID {uid} RFC822.SIZE {raw_bytes} FLAGS () BODY[HEADER.FIELDS (FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES)] {{{}}}\r\n{head})\r\n", head.len()));
                                } else {
                                    // The fixture's total raw size includes headers. Keep at the protocol's 2 MiB boundary.
                                    let mut raw = head;
                                    raw.push_str(&"x".repeat(raw_bytes - raw.len()));
                                    out.push_str(&format!("* 1 FETCH (UID {uid} BODY[] {{{}}}\r\n{raw})\r\n", raw.len()));
                                }
                            }
                            out.push_str(&format!("{tag} OK fetch\r\n"));
                            out
                        } else if command.starts_with("LOGOUT") {
                            format!("* BYE done\r\n{tag} OK logout\r\n")
                        } else {
                            format!("{tag} BAD unsupported\r\n")
                        };
                        if io.get_mut().write_all(reply.as_bytes()).await.is_err() { break }
                    }
                }
            });
        });
        Self {
            port,
            root_pem,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
