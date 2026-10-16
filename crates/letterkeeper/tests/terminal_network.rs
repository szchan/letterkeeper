// SPDX-License-Identifier: GPL-3.0-only
//! Real PTY driver over the shared TLS IMAP fixture. No real vault is used.
use letterkeeper_accounts::*;
use letterkeeper_test_support::mail as support;
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::process::CommandExt,
    sync::Arc,
    time::{Duration, Instant},
};
struct Vault;
impl CredentialStore for Vault {
    fn get(&self, _account: &str, _kind: SecretKind) -> Result<Option<Secret>, AccountError> {
        Ok(Some(Secret::new("synthetic-secret".into())))
    }
    fn set(&self, _account: &str, _kind: SecretKind, _secret: &Secret) -> Result<(), AccountError> {
        Ok(())
    }
    fn delete(&self, _account: &str, _kind: SecretKind) -> Result<(), AccountError> {
        Ok(())
    }
}
#[test]
fn network_tui_child() {
    let Some(path) = std::env::var_os("LETTERKEEPER_TEST_TUI_CONFIG") else {
        return;
    };
    let config: AccountConfig = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let state = std::path::Path::new(&path).parent().unwrap().join("state");
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: env!("CARGO_BIN_EXE_letterkeeper").into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host =
        letterkeeper::remote::RemoteHost::open(config, &state, Arc::new(Vault), sandbox).unwrap();
    letterkeeper_adapters::run_tui(Arc::new(host)).unwrap();
}
struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn plain(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < bytes.len() {
                let b = bytes[i];
                i += 1;
                if (0x40..=0x7e).contains(&b) {
                    break;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}
fn receive(master: &mut File, needle: &[u8]) -> Vec<u8> {
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    // Crossterm can represent blank cells using cursor moves instead of spaces.
    let marker = needle
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect::<Vec<_>>();
    while start.elapsed() < Duration::from_secs(12) {
        match master.read(&mut buffer) {
            Ok(n) if n > 0 => {
                bytes.extend_from_slice(&buffer[..n]);
                let text = plain(&bytes)
                    .into_iter()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect::<Vec<_>>();
                if text.windows(marker.len()).any(|w| w == marker) {
                    return bytes;
                }
                assert!(bytes.len() < 256 * 1024);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            _ => (),
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "PTY did not display synthetic marker; {} bytes observed",
        bytes.len()
    );
}
#[test]
fn real_tls_terminal_reads_on_demand_resizes_and_restores_terminal() {
    let server = support::Server::start_shared();
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, server.tls.roots_pem.as_ref().unwrap()).unwrap();
    let config = dir.path().join("account.json");
    let config_value = serde_json::json!({"id":"controlled","email":"person@example.test","username":"person@example.test","imap":{"host":"localhost","port":server.endpoint.port},"smtp":{"host":"localhost","port":9465},"allowed_folders":["INBOX"],"permissions":{"read":true,"draft":false,"send":false},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed","authentication":{"type":"password"},"extra_ca_pem":ca});
    std::fs::write(&config, serde_json::to_vec(&config_value).unwrap()).unwrap();
    let (mut master_fd, mut slave_fd) = (-1, -1);
    let size = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: valid writable FD pointers and a fully initialized winsize; null termios requests defaults.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        },
        0
    );
    // SAFETY: openpty created two owned descriptors, transferred exactly once to File.
    let (mut master, slave) =
        unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
    let mut before = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: live slave FD and writable termios storage; initialized only on success.
    assert_eq!(
        unsafe { libc::tcgetattr(slave.as_raw_fd(), before.as_mut_ptr()) },
        0
    );
    // SAFETY: tcgetattr above succeeded.
    let before = unsafe { before.assume_init() };
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "network_tui_child", "--nocapture"])
        .env("LETTERKEEPER_TEST_TUI_CONFIG", &config)
        .env("TERM", "xterm-256color")
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave.try_clone().unwrap());
    // SAFETY: post-fork closure only uses async-signal-safe session/ioctl syscalls and fixed errors.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    // SAFETY: live master FD; setting O_NONBLOCK changes only this owned PTY endpoint.
    assert_ne!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
        -1
    );
    receive(&mut master, b"Synthetic thread");
    master.write_all(b"\r").unwrap();
    let output = receive(&mut master, b"synthetic body");
    assert!(
        !output
            .windows(b"resource.invalid".len())
            .any(|w| w == b"resource.invalid")
    );
    let small = libc::winsize {
        ws_row: 12,
        ws_col: 24,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: live slave descriptor and initialized winsize; SIGWINCH targets our child only.
    assert_eq!(
        unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &small) },
        0
    );
    assert_eq!(
        unsafe { libc::kill(child.0.id() as i32, libc::SIGWINCH) },
        0
    );
    master.write_all(b"?q").unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut after = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: live slave descriptor and writable termios storage.
    assert_eq!(
        unsafe { libc::tcgetattr(slave.as_raw_fd(), after.as_mut_ptr()) },
        0
    );
    // SAFETY: tcgetattr above succeeded.
    let after = unsafe { after.assume_init() };
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
    let transcript = server.finish();
    assert!(transcript.iter().any(|line| line.contains("BODY.PEEK[]")));
    assert!(!transcript.iter().any(|line| line.contains("UID STORE")));
}
