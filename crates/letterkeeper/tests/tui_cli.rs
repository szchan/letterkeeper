// SPDX-License-Identifier: GPL-3.0-only
use std::process::{Command, Stdio};
#[test]
fn tui_help_exposes_read_only_and_explicit_synthetic_transport() {
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .args(["tui", "--help"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(text.contains("--read-only"));
    assert!(text.contains("--smtp-test"));
}
#[test]
fn noninteractive_tui_rejects_before_reading_mail_or_creating_storage() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("private");
    let result = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--state-dir")
        .arg(&state)
        .args(["tui", "--mail", "/does-not-exist/synthetic.eml"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("interactive_terminal_required")
    );
    assert!(!state.exists());
}

#[test]
fn draft_only_tui_starts_without_eml_input() {
    use std::{
        fs::File,
        io::{Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::process::CommandExt,
        },
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("private");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/draft.json")
        .canonicalize()
        .unwrap();
    let created = Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--state-dir")
        .arg(&state)
        .args(["draft", "create"])
        .arg(fixture)
        .output()
        .unwrap();
    assert!(created.status.success());
    let (mut master, mut slave) = (-1, -1);
    let dimensions = libc::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: two writable descriptor slots and an initialized dimensions struct.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &dimensions,
            )
        },
        0
    );
    // SAFETY: own both fresh descriptors returned by openpty exactly once.
    let (mut master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
    // SAFETY: live owned descriptor; preserving flags while adding nonblocking.
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        assert!(flags >= 0);
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_letterkeeper"));
    command
        .arg("--state-dir")
        .arg(&state)
        .arg("tui")
        .env("TERM", "xterm-256color")
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave.try_clone().unwrap());
    // SAFETY: post-fork closure only invokes async-signal-safe session/ioctl calls.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    struct Guard(std::process::Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Guard(command.spawn().unwrap());
    let mut bytes = Vec::new();
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        let mut block = [0; 8192];
        match master.read(&mut block) {
            Ok(n) => bytes.extend_from_slice(&block[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
        if bytes
            .windows(b"LetterKeeper".len())
            .any(|w| w == b"LetterKeeper")
        {
            master.write_all(b"q").unwrap();
            assert!(child.0.wait().unwrap().success());
            return;
        }
        if child.0.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "draft-only TUI never entered its workspace: {}",
        String::from_utf8_lossy(&bytes)
    );
}
