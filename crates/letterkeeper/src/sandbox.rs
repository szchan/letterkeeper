// SPDX-License-Identifier: GPL-3.0-only
//! Linux worker launcher. Failure never falls back to in-process parsing.
use letterkeeper_content::{Limits, SafeDocument, parse_message};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_OUTPUT: u64 = 4 * 1024 * 1024;
const MARKER: &[u8] = b"letterkeeper-isolated-v1";

pub struct Sandbox {
    pub executable: PathBuf,
    pub launcher: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProbeReport {
    pub http_connected: bool,
    pub dns_reply: bool,
    pub host_file_readable: bool,
    pub parsed: bool,
}

impl Sandbox {
    pub fn current() -> Result<Self, String> {
        Ok(Self {
            executable: std::env::current_exe().map_err(|_| "executable_unavailable")?,
            launcher: PathBuf::from("/usr/bin/bwrap"),
        })
    }

    pub fn parse(&self, raw: &[u8]) -> Result<SafeDocument, String> {
        let output = self.run("__worker", &[], raw)?;
        serde_json::from_slice(&output).map_err(|_| "invalid_worker_output".into())
    }
    pub fn inspect(&self, _raw: &[u8]) -> Result<letterkeeper_content::InspectedMail, String> {
        Err("inspect_not_implemented".into())
    }
    pub fn attachment(&self, _raw: &[u8], _id: usize) -> Result<letterkeeper_content::DecodedAttachment, String> {
        Err("attachment_not_implemented".into())
    }

    pub fn probe(
        &self,
        raw: &[u8],
        http: SocketAddr,
        dns: SocketAddr,
        sentinel: &Path,
    ) -> Result<ProbeReport, String> {
        if !http.ip().is_loopback() || !dns.ip().is_loopback() {
            return Err("probe_requires_loopback".into());
        }
        let args = vec![
            http.to_string(),
            dns.to_string(),
            sentinel.to_str().ok_or("invalid_path")?.to_owned(),
        ];
        serde_json::from_slice(&self.run("__probe", &args, raw)?)
            .map_err(|_| "invalid_probe_output".into())
    }

    fn run(&self, mode: &str, args: &[String], raw: &[u8]) -> Result<Vec<u8>, String> {
        if raw.len() > Limits::default().max_raw_bytes {
            return Err("message_too_large".into());
        }
        let executable = self
            .executable
            .canonicalize()
            .map_err(|_| "executable_unavailable")?;
        let mut marker = tempfile::NamedTempFile::new().map_err(|_| "sandbox_setup_failed")?;
        marker
            .write_all(MARKER)
            .map_err(|_| "sandbox_setup_failed")?;
        let mut command = Command::new(&self.launcher);
        command
            .args([
                "--unshare-all",
                "--die-with-parent",
                "--new-session",
                "--cap-drop",
                "ALL",
                "--ro-bind",
                "/usr",
                "/usr",
                "--symlink",
                "usr/lib",
                "/lib",
                "--symlink",
                "usr/lib",
                "/lib64",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--tmpfs",
                "/tmp",
                "--dir",
                "/app",
            ])
            .arg("--ro-bind")
            .arg(executable)
            .arg("/app/letterkeeper")
            .arg("--ro-bind")
            .arg(marker.path())
            .arg("/letterkeeper-isolated")
            .args([
                "--chdir",
                "/app",
                "--clearenv",
                "--setenv",
                "LANG",
                "C.UTF-8",
                "/app/letterkeeper",
                mode,
            ])
            .args(args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // SAFETY: the post-fork closure only calls async-signal-safe setrlimit and
        // constructs an OS error on failure. No allocation/locks on success.
        unsafe {
            command.pre_exec(|| {
                for (resource, value) in [
                    (libc::RLIMIT_AS, 512 * 1024 * 1024),
                    (libc::RLIMIT_CPU, 5),
                    (libc::RLIMIT_FSIZE, 0),
                    (libc::RLIMIT_NOFILE, 64),
                ] {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|_| "sandbox_unavailable")?;
        let mut input = child.stdin.take().ok_or("worker_input_unavailable")?;
        let output = child.stdout.take().ok_or("worker_output_unavailable")?;
        let bytes = raw.to_vec();
        let writer = std::thread::spawn(move || input.write_all(&bytes));
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            output
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if started.elapsed() < Duration::from_secs(8) => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("worker_timeout_or_failure");
                }
            }
        };
        let written = writer.join().map_err(|_| "worker_write_failed")?;
        let bytes = reader
            .join()
            .map_err(|_| "worker_read_failed")?
            .map_err(|_| "worker_read_failed")?;
        if !status?.success() || written.is_err() || bytes.len() as u64 > MAX_OUTPUT {
            return Err("sandbox_or_parse_failed".into());
        }
        Ok(bytes)
    }
}

/// Internal entry point. The root marker is provided only in the constructed
/// mount namespace; this guard also prevents accidental direct CLI use.
pub fn worker_entry(args: &[String]) -> Result<(), String> {
    if std::fs::read("/letterkeeper-isolated").ok().as_deref() != Some(MARKER) {
        return Err("worker_requires_isolation".into());
    }
    let mut raw = Vec::new();
    std::io::stdin()
        .take(Limits::default().max_raw_bytes as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|_| "worker_read_failed")?;
    let doc = parse_message(&raw, Limits::default()).map_err(|_| "parse_rejected")?;
    if args.first().map(String::as_str) == Some("__probe") {
        if args.len() != 4 {
            return Err("invalid_probe".into());
        }
        let http: SocketAddr = args[1].parse().map_err(|_| "invalid_probe")?;
        let dns: SocketAddr = args[2].parse().map_err(|_| "invalid_probe")?;
        if !http.ip().is_loopback() || !dns.ip().is_loopback() {
            return Err("invalid_probe".into());
        }
        let http_connected = TcpStream::connect_timeout(&http, Duration::from_millis(150)).is_ok();
        let bind = if dns.is_ipv4() {
            "127.0.0.1:0"
        } else {
            "[::1]:0"
        };
        let dns_reply = UdpSocket::bind(bind).is_ok_and(|socket| {
            let _ = socket.set_read_timeout(Some(Duration::from_millis(150)));
            // Synthetic DNS query for tracker.example.test; no real user data.
            let query = b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07tracker\x07example\x04test\x00\x00\x01\x00\x01";
            socket.send_to(query, dns).is_ok() && socket.recv_from(&mut [0; 512]).is_ok()
        });
        let report = ProbeReport {
            http_connected,
            dns_reply,
            host_file_readable: std::fs::read(&args[3]).is_ok(),
            parsed: !doc.text.is_empty(),
        };
        serde_json::to_writer(std::io::stdout(), &report).map_err(|_| "worker_output_failed")?;
    } else {
        serde_json::to_writer(std::io::stdout(), &doc).map_err(|_| "worker_output_failed")?;
    }
    Ok(())
}
