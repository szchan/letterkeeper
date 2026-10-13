// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::files::{private_lock, read_regular};
use std::time::Duration;

#[test]
fn child_lock_probe() {
    let Some(path) = std::env::var_os("LETTERKEEPER_TEST_LOCK_DIRECTORY") else {
        return;
    };
    let result = private_lock(std::path::Path::new(&path), "send.lock", Duration::ZERO);
    if std::env::var_os("LETTERKEEPER_TEST_EXPECT_BUSY").is_some() {
        assert!(result.is_err());
    } else {
        assert!(result.is_ok());
    }
}

#[test]
fn operating_system_lock_excludes_other_processes_and_releases_on_drop() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let lock = private_lock(dir.path(), "send.lock", Duration::ZERO).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "child_lock_probe"])
        .env("LETTERKEEPER_TEST_LOCK_DIRECTORY", dir.path())
        .env("LETTERKEEPER_TEST_EXPECT_BUSY", "1");
    assert!(child.output().unwrap().status.success());
    drop(lock);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", "child_lock_probe"])
        .env("LETTERKEEPER_TEST_LOCK_DIRECTORY", dir.path());
    assert!(child.output().unwrap().status.success());
}

#[test]
fn special_files_and_symlink_ancestors_are_rejected_without_blocking() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("ca.pem");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o600),
        0,
    )
    .unwrap();
    let started = std::time::Instant::now();
    assert!(read_regular(&fifo, 65536).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    let data = dir.path().join("private");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("file"), b"synthetic").unwrap();
    let alias = dir.path().join("alias");
    symlink(data, &alias).unwrap();
    assert!(read_regular(&alias.join("file"), 65536).is_err());
    let unsafe_lock = dir.path().join("send.lock");
    symlink(&fifo, &unsafe_lock).unwrap();
    assert!(private_lock(dir.path(), "send.lock", Duration::ZERO).is_err());
}
