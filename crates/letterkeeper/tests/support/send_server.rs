// SPDX-License-Identifier: GPL-3.0-only
//! Loopback TLS SMTP with deliberately sensitive synthetic rejection text.
#![allow(dead_code)]
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
pub enum ResultMode {
    Accepted,
    Rejected,
    Disconnected,
}
pub struct Server {
    pub port: u16,
    pub pem: Vec<u8>,
    pub transcript: Arc<Mutex<Vec<String>>>,
    thread: std::thread::JoinHandle<()>,
}
pub const PRIVATE_REPLY: &str =
    "private-server-detail raw-error-address@example.test synthetic-secret";
impl Server {
    pub fn start(mode: ResultMode) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let transcript = Arc::new(Mutex::new(Vec::new()));
        let observed = transcript.clone();
        let thread = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(20);
            let tcp = loop {
                match listener.accept() {
                    Ok((tcp, _)) => break tcp,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < until, "synthetic SMTP was never contacted");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => panic!("synthetic SMTP listener failed"),
                }
            };
            tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            tcp.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            let stream = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                tcp,
            );
            let mut io = BufReader::new(stream);
            io.get_mut()
                .write_all(b"220 synthetic.test ready\r\n")
                .unwrap();
            io.get_mut().flush().unwrap();
            loop {
                let mut line = String::new();
                if io.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                observed.lock().unwrap().push(line.clone());
                let reply = if line.starts_with("EHLO") {
                    "250-synthetic.test\r\n250-AUTH PLAIN\r\n250 8BITMIME\r\n".to_string()
                } else if line.starts_with("AUTH") {
                    "235 authenticated\r\n".into()
                } else if line.starts_with("MAIL") || line.starts_with("RCPT") {
                    "250 ok\r\n".into()
                } else if line.starts_with("DATA") {
                    io.get_mut().write_all(b"354 data\r\n").unwrap();
                    io.get_mut().flush().unwrap();
                    let mut body = String::new();
                    loop {
                        let mut part = String::new();
                        if io.read_line(&mut part).unwrap_or(0) == 0 {
                            return;
                        }
                        if part == ".\r\n" {
                            break;
                        }
                        body.push_str(&part);
                    }
                    observed.lock().unwrap().push(body);
                    match mode {
                        ResultMode::Accepted => "250 accepted\r\n".into(),
                        ResultMode::Rejected => format!("550 {PRIVATE_REPLY}\r\n"),
                        ResultMode::Disconnected => break,
                    }
                } else if line.starts_with("QUIT") {
                    break;
                } else {
                    panic!("unexpected synthetic SMTP command");
                };
                if io.get_mut().write_all(reply.as_bytes()).is_err() {
                    break;
                }
                if io.get_mut().flush().is_err() {
                    break;
                }
            }
        });
        Self {
            port,
            pem,
            transcript,
            thread,
        }
    }
    pub fn finish(self) -> Vec<String> {
        self.thread.join().unwrap();
        self.transcript.lock().unwrap().clone()
    }
}
