// SPDX-License-Identifier: GPL-3.0-only
//! Real PTY smoke for the human workspace. Child uses synthetic local state only.
use letterkeeper::app::AppHost;
use letterkeeper_adapters::{Host, HostError, MessageSummary, DraftInput as AgentDraft, run_tui_workspace, workspace::{HumanWorkspace, WorkspaceCapabilities}};
use letterkeeper_content::SafeDocument;
use serde_json::Value;
use std::{
    fs::File,
    io::{Read, Write},
    os::{fd::{AsRawFd, FromRawFd}, unix::process::CommandExt},
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn workspace_pty_child() {
    let Some(path) = std::env::var_os("LETTERKEEPER_WORKSPACE_TEST_STATE") else { return };
    let host = Arc::new(AppHost::open(std::path::Path::new(&path), vec![], false, None).unwrap());
    let browser: Arc<dyn Host> = host.clone();
    let human: Arc<dyn HumanWorkspace> = host;
    run_tui_workspace(browser, human).unwrap();
}

struct BenchmarkHost { messages: Vec<MessageSummary>, body_bytes: usize }
impl Host for BenchmarkHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self.messages.iter().skip(offset).take(limit.min(50)).cloned().collect())
    }
    fn search_messages(&self, _: Option<&str>, query: &str, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self.messages.iter().filter(|m| m.subject.contains(query)).skip(offset).take(limit.min(50)).cloned().collect())
    }
    fn read_message(&self, id: &str) -> Result<SafeDocument, HostError> {
        if !self.messages.iter().any(|m| m.id == id) { return Err(HostError("missing".into())) }
        Ok(SafeDocument { subject: id.into(), from: "sender@example.test".into(), text: "中".repeat(self.body_bytes / 3),
            links: vec![], attachments: vec![], truncated: false, warnings: vec![] })
    }
    fn create_draft(&self, _: AgentDraft) -> Result<Value, HostError> { Err(HostError("unsupported".into())) }
    fn get_draft(&self, _: &str) -> Result<Value, HostError> { Err(HostError("unsupported".into())) }
    fn send_draft(&self, _: &str) -> Result<Value, HostError> { Err(HostError("unsupported".into())) }
    fn send_status(&self, _: &str) -> Result<Value, HostError> { Err(HostError("unsupported".into())) }
}
struct BenchmarkWorkspace;
impl HumanWorkspace for BenchmarkWorkspace {
    fn capabilities(&self) -> WorkspaceCapabilities {
        WorkspaceCapabilities { account: "benchmark".into(), from: "sender@example.test".into(), read: true, draft: false, send: false }
    }
}
#[test]
fn workspace_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_COUNT") else { return };
    let count: usize = count.parse().unwrap();
    assert!(count == 1_000 || count == 10_000);
    let body_bytes = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES").unwrap().parse().unwrap();
    assert!(body_bytes == 32 * 1024 || body_bytes == 2 * 1024 * 1024);
    let messages = (0..count).map(|i| MessageSummary {
        id: format!("mail-{i}"), subject: format!("Synthetic subject {i:05}"), from: "sender@example.test".into(),
    }).collect();
    let host: Arc<dyn Host> = Arc::new(BenchmarkHost { messages, body_bytes });
    let human: Arc<dyn HumanWorkspace> = Arc::new(BenchmarkWorkspace);
    run_tui_workspace(host, human).unwrap();
}

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() { let _ = self.0.kill(); }
        let _ = self.0.wait();
    }
}
fn recv(master: &mut File, marker: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buf = [0u8; 8192];
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        match master.read(&mut buf) {
            Ok(n) if n > 0 => output.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            _ => (),
        }
        if String::from_utf8_lossy(&output).contains(marker) { return output }
        assert!(output.len() < 512 * 1024, "terminal generated unbounded output");
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("PTY missing marker {marker:?}; captured {} bytes", output.len());
}
fn size(slave: &File, width: u16, height: u16) {
    let winsize = libc::winsize { ws_row: height, ws_col: width, ws_xpixel: 0, ws_ypixel: 0 };
    // SAFETY: valid slave PTY descriptor and initialized winsize.
    assert_eq!(unsafe { libc::ioctl(slave.as_raw_fd(), libc::TIOCSWINSZ, &winsize) }, 0);
}

#[test]
fn workspace_four_sizes_editing_q_and_terminal_restore() {
    for (width, height) in [(120, 40), (80, 24), (40, 12), (20, 6)] {
        let dir = tempfile::tempdir().unwrap();
        let (mut master_fd, mut slave_fd) = (-1, -1);
        let winsize = libc::winsize { ws_row: height, ws_col: width, ws_xpixel: 0, ws_ypixel: 0 };
        // SAFETY: pointers refer to valid storage and winsize is initialized.
        assert_eq!(unsafe { libc::openpty(&mut master_fd, &mut slave_fd, std::ptr::null_mut(), std::ptr::null(), &winsize) }, 0);
        // SAFETY: openpty gave us two owned FDs, each transferred once.
        let (mut master, slave) = unsafe { (File::from_raw_fd(master_fd), File::from_raw_fd(slave_fd)) };
        let before = unsafe {
            let mut value = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), value.as_mut_ptr()), 0);
            value.assume_init()
        };
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "workspace_pty_child", "--nocapture"])
            .env("LETTERKEEPER_WORKSPACE_TEST_STATE", dir.path())
            .env("TERM", "xterm-256color")
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave.try_clone().unwrap());
        // SAFETY: after fork, only async-signal-safe session and ioctl calls are used.
        unsafe { command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }); }
        let mut child = Child(command.spawn().unwrap());
        // SAFETY: owned live PTY master descriptor.
        assert_ne!(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) }, -1);
        recv(&mut master, "LetterKeeper");
        master.write_all(b"c").unwrap();
        recv(&mut master, "编辑");
        master.write_all(b"q").unwrap();
        assert!(child.0.try_wait().unwrap().is_none(), "q must be text inside the editor");
        size(&slave, width.saturating_sub(1).max(10), height.saturating_sub(1).max(4));
        // SAFETY: child owns this PTY session and SIGWINCH only requests redraw.
        assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGWINCH) }, 0);
        master.write_all(b"\x1b").unwrap();
        recv(&mut master, "保存");
        master.write_all(b"d").unwrap();
        master.write_all(b"q").unwrap();
        let start = Instant::now();
        while child.0.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(5), "terminal did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
        let after = unsafe {
            let mut value = std::mem::MaybeUninit::<libc::termios>::uninit();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), value.as_mut_ptr()), 0);
            value.assume_init()
        };
        assert_eq!((after.c_iflag, after.c_oflag, after.c_cflag, after.c_lflag, after.c_cc),
                   (before.c_iflag, before.c_oflag, before.c_cflag, before.c_lflag, before.c_cc));
    }
}
