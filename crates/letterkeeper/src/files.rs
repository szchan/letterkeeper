// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::Attachment;
use rustix::fs::{self as fs, FileType, Mode, OFlags};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::OwnedFd,
    path::{Component, Path},
    time::{Duration, Instant},
};
const MAX_BYTES: usize = letterkeeper_content::MAX_ATTACHMENT_BYTES;
fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\'])
        && letterkeeper_content::sanitize_text(name) == name
        && !name.chars().any(char::is_control)
}
pub fn open_directory(path: &Path) -> Result<OwnedFd, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "directory_unavailable")?
            .join(path)
    };
    let mut fd = fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| "directory_unavailable")?;
    for component in path.components() {
        match component {
            Component::RootDir => (),
            Component::Normal(part) => {
                fd = fs::openat(
                    &fd,
                    part,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| "directory_unavailable")?
            }
            _ => return Err("unsafe_directory".into()),
        }
    }
    Ok(fd)
}
pub fn import_attachment(path: &Path, content_type: &str) -> Result<Attachment, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| name_ok(n))
        .ok_or("unsafe_attachment_name")?;
    if content_type.len() > 127
        || content_type.split('/').count() != 2
        || content_type.split('/').any(str::is_empty)
        || !content_type
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-/".contains(&b))
    {
        return Err("invalid_attachment_type".into());
    }
    let parent = open_directory(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let fd = fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| "attachment_file_unavailable")?;
    let stat = fs::fstat(&fd).map_err(|_| "attachment_file_unavailable")?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_nlink != 1
        || stat.st_size < 0
        || stat.st_size as usize > MAX_BYTES
    {
        return Err("unsafe_or_oversized_attachment".into());
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "attachment_read_failed")?;
    if bytes.len() > MAX_BYTES {
        return Err("attachment_too_large".into());
    }
    Ok(Attachment {
        name: name.into(),
        content_type: content_type.into(),
        bytes,
    })
}
pub fn read_regular(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let name = path.file_name().ok_or("input_file_unavailable")?;
    let parent = open_directory(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let fd = fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| "input_file_unavailable")?;
    let stat = fs::fstat(&fd).map_err(|_| "input_file_unavailable")?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_size < 0
        || stat.st_size as u64 > max as u64
    {
        return Err("input_file_unsafe_or_too_large".into());
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_file_unavailable")?;
    if bytes.len() > max {
        return Err("input_too_large".into());
    }
    Ok(bytes)
}
pub fn export_attachment(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    if !name_ok(name) || bytes.len() > MAX_BYTES {
        return Err("unsafe_or_oversized_attachment".into());
    }
    let parent = open_directory(directory)?;
    let stat = fs::fstat(&parent).map_err(|_| "unsafe_directory")?;
    if stat.st_mode & 0o022 != 0 || stat.st_uid != rustix::process::geteuid().as_raw() {
        return Err("unsafe_export_directory".into());
    }
    let fd = fs::openat(
        &parent,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| "attachment_destination_exists_or_unavailable")?;
    let result = (|| {
        let mut file = File::from(fd);
        file.write_all(bytes)
            .map_err(|_| "attachment_write_failed")?;
        file.sync_all().map_err(|_| "attachment_write_failed")?;
        fs::fsync(&parent).map_err(|_| "attachment_write_failed")
    })()
    .map_err(str::to_owned);
    if result.is_err() {
        let _ = fs::unlinkat(&parent, name, fs::AtFlags::empty());
    }
    result
}
/// Advisory process lock. The opened descriptor owns the lock until dropped.
pub fn private_lock(directory: &Path, name: &str, wait: Duration) -> Result<File, String> {
    if !name_ok(name) {
        return Err("invalid_lock_name".into());
    }
    let parent = open_directory(directory)?;
    let stat = fs::fstat(&parent).map_err(|_| "unsafe_lock_directory")?;
    if stat.st_mode & 0o7777 != 0o700 || stat.st_uid != rustix::process::geteuid().as_raw() {
        return Err("unsafe_lock_directory".into());
    }
    let fd = fs::openat(
        parent,
        name,
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::from_raw_mode(0o600),
    )
    .map_err(|_| "unsafe_lock_file")?;
    let stat = fs::fstat(&fd).map_err(|_| "unsafe_lock_file")?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_nlink != 1
        || stat.st_mode & 0o7777 != 0o600
        || stat.st_uid != rustix::process::geteuid().as_raw()
    {
        return Err("unsafe_lock_file".into());
    }
    let start = Instant::now();
    loop {
        match fs::flock(&fd, fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(File::from(fd)),
            Err(e) if e == rustix::io::Errno::WOULDBLOCK && start.elapsed() < wait => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => return Err("account_operation_busy".into()),
        }
    }
}
