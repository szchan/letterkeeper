// SPDX-License-Identifier: GPL-3.0-only
#![allow(dead_code)]
use letterkeeper_protocol::{Credentials, Endpoint, TlsOptions};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
#[derive(Clone, Copy)]
pub enum Scenario {
    Imap,
    HugeLiteral,
    AuthReject,
    AppendDisconnect,
    AppendReject,
    Smtp,
    SmtpReject,
    SmtpDisconnect,
}
pub struct Server {
    pub endpoint: Endpoint,
    pub tls: TlsOptions,
    pub transcript: Arc<Mutex<Vec<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
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
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                let Ok(Ok((tcp, _))) =
                    tokio::time::timeout(std::time::Duration::from_secs(3), listener.accept())
                        .await
                else {
                    return;
                };
                let Ok(Ok(tls)) = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    tokio_rustls::TlsAcceptor::from(Arc::new(config)).accept(tcp),
                )
                .await
                else {
                    return;
                };
                let mut io = BufReader::new(tls);
                let smtp = matches!(
                    scenario,
                    Scenario::Smtp | Scenario::SmtpReject | Scenario::SmtpDisconnect
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
                        imap_reply(&mut io, &line, scenario, &observed).await
                    };
                    if !result {
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
        format!("* SEARCH 11 12 13\r\n{tag} OK search\r\n")
    } else if cmd.starts_with("UID FETCH") {
        if matches!(scenario, Scenario::HugeLiteral) {
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
            format!("{tag} OK append\r\n")
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
        "250 accepted\r\n"
    } else if line.starts_with("QUIT") {
        let _ = io.get_mut().write_all(b"221 bye\r\n").await;
        return false;
    } else {
        panic!("unexpected synthetic SMTP command")
    };
    io.get_mut().write_all(reply.as_bytes()).await.is_ok()
}
