// SPDX-License-Identifier: GPL-3.0-only
//! Read-only environment diagnostics; no credential items are inspected.
use rustix::fs::{self as fs, FileType, Mode, OFlags};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageStatus {
    Ready,
    NotInitialized,
    Unsafe,
}

/// Inspect descriptors without creating directories or reading their contents.
pub fn inspect_private_directory(path: &Path) -> StorageStatus {
    inspect_directory_owned_by(path, rustix::process::geteuid().as_raw())
}

fn inspect_directory_owned_by(path: &Path, uid: u32) -> StorageStatus {
    if !path.is_absolute()
        || path.as_os_str().len() > 4096
        || path.components().count() > 128
        || path
            .components()
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
    {
        return StorageStatus::Unsafe;
    }
    let Ok(mut fd) = fs::open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    ) else {
        return StorageStatus::Unsafe;
    };
    for part in path.components() {
        if let Component::Normal(name) = part {
            fd = match fs::openat(
                &fd,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) => return StorageStatus::NotInitialized,
                Err(_) => return StorageStatus::Unsafe,
            };
        }
    }
    match fs::fstat(&fd) {
        Ok(stat)
            if FileType::from_raw_mode(stat.st_mode) == FileType::Directory
                && stat.st_mode & 0o7777 == 0o700
                && stat.st_uid == uid =>
        {
            StorageStatus::Ready
        }
        _ => StorageStatus::Unsafe,
    }
}

pub fn inspect_runtime_directory() -> StorageStatus {
    inspect_private_directory(&runtime_directory())
}

fn runtime_directory() -> PathBuf {
    PathBuf::from(format!("/run/user/{}", rustix::process::geteuid().as_raw()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialServiceStatus {
    Unlocked,
    Locked,
    NotInitialized,
    Unavailable,
    Unknown,
    TimedOut,
}

/// Only ReadAlias(default) and Properties.Get(Collection.Locked), with no
/// service activation, session negotiation, items, unlock, or prompts.
pub fn inspect_credential_service() -> CredentialServiceStatus {
    let runtime = runtime_directory();
    if inspect_private_directory(&runtime) != StorageStatus::Ready {
        return CredentialServiceStatus::Unknown;
    }
    let address = match std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        Ok(address) if valid_local_bus_address(&address) => Some(address),
        Ok(_) | Err(std::env::VarError::NotUnicode(_)) => return CredentialServiceStatus::Unknown,
        Err(std::env::VarError::NotPresent) => None,
    };
    inspect_service_with(
        Path::new("/usr/bin/busctl"),
        address.as_deref(),
        &runtime,
        Duration::from_secs(3),
    )
}

fn valid_local_bus_address(address: &str) -> bool {
    if address.len() > 4096 {
        return false;
    }
    let Some(value) = address
        .strip_prefix("unix:path=")
        .or_else(|| address.strip_prefix("unix:abstract="))
    else {
        return false;
    };
    let mut fields = value.split(',');
    let Some(name) = fields.next().filter(|s| !s.is_empty()) else {
        return false;
    };
    if address.starts_with("unix:path=") && !name.starts_with('/') {
        return false;
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/_-.:".contains(&b))
    {
        return false;
    }
    match (fields.next(), fields.next()) {
        (None, None) => true,
        (Some(guid), None) => guid
            .strip_prefix("guid=")
            .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        _ => false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BusReply<T> {
    #[serde(rename = "type")]
    signature: String,
    data: T,
}

fn collection_path(bytes: &[u8]) -> Result<Option<String>, CredentialServiceStatus> {
    let reply: BusReply<Vec<String>> =
        serde_json::from_slice(bytes).map_err(|_| CredentialServiceStatus::Unknown)?;
    if reply.signature != "o" || reply.data.len() != 1 {
        return Err(CredentialServiceStatus::Unknown);
    }
    let path = &reply.data[0];
    if path == "/" {
        return Ok(None);
    }
    let Some(suffix) = path.strip_prefix("/org/freedesktop/secrets/collection/") else {
        return Err(CredentialServiceStatus::Unknown);
    };
    if path.len() > 1024
        || suffix.is_empty()
        || !suffix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(CredentialServiceStatus::Unknown);
    }
    Ok(Some(path.clone()))
}

fn locked_status(bytes: &[u8]) -> CredentialServiceStatus {
    let Ok(reply) = serde_json::from_slice::<BusReply<Vec<BusReply<bool>>>>(bytes) else {
        return CredentialServiceStatus::Unknown;
    };
    if reply.signature != "v" || reply.data.len() != 1 || reply.data[0].signature != "b" {
        return CredentialServiceStatus::Unknown;
    }
    if reply.data[0].data {
        CredentialServiceStatus::Locked
    } else {
        CredentialServiceStatus::Unlocked
    }
}

fn inspect_service_with(
    executable: &Path,
    address: Option<&str>,
    runtime: &Path,
    budget: Duration,
) -> CredentialServiceStatus {
    if address.is_some_and(|value| !valid_local_bus_address(value)) {
        return CredentialServiceStatus::Unknown;
    }
    let deadline = Instant::now() + budget;
    let alias = match bus_call(
        executable,
        address,
        runtime,
        &[
            "call",
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets",
            "org.freedesktop.Secret.Service",
            "ReadAlias",
            "s",
            "default",
        ],
        deadline,
    ) {
        Ok(bytes) => bytes,
        Err(status) => return status,
    };
    let collection = match collection_path(&alias) {
        Ok(Some(path)) => path,
        Ok(None) => return CredentialServiceStatus::NotInitialized,
        Err(status) => return status,
    };
    match bus_call(
        executable,
        address,
        runtime,
        &[
            "call",
            "org.freedesktop.secrets",
            &collection,
            "org.freedesktop.DBus.Properties",
            "Get",
            "ss",
            "org.freedesktop.Secret.Collection",
            "Locked",
        ],
        deadline,
    ) {
        Ok(bytes) => locked_status(&bytes),
        Err(status) => status,
    }
}

fn bus_call(
    executable: &Path,
    address: Option<&str>,
    runtime: &Path,
    args: &[&str],
    deadline: Instant,
) -> Result<Vec<u8>, CredentialServiceStatus> {
    if Instant::now() >= deadline {
        return Err(CredentialServiceStatus::TimedOut);
    }
    #[cfg(not(test))]
    let mut command = Command::new(executable);
    #[cfg(test)]
    let mut command = tests::fixture_command(executable);
    command
        .args([
            "--user",
            "--auto-start=no",
            "--allow-interactive-authorization=no",
            "--json=short",
            "--no-pager",
            "--timeout=3s",
        ])
        .args(args)
        .env_clear()
        .env("XDG_RUNTIME_DIR", runtime)
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(address) = address {
        command.env("DBUS_SESSION_BUS_ADDRESS", address);
    }
    let mut child = command.spawn().map_err(|error| {
        #[cfg(test)]
        eprintln!(
            "synthetic_fixture_spawn_error kind={:?} errno={:?}",
            error.kind(),
            error.raw_os_error()
        );
        #[cfg(not(test))]
        let _ = error;
        CredentialServiceStatus::Unavailable
    })?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or(CredentialServiceStatus::Unknown)?;
        let flags = fs::fcntl_getfl(&stdout).map_err(|_| CredentialServiceStatus::Unknown)?;
        fs::fcntl_setfl(&stdout, flags | OFlags::NONBLOCK)
            .map_err(|_| CredentialServiceStatus::Unknown)?;
        let mut bytes = Vec::new();
        let mut ended = false;
        loop {
            if Instant::now() >= deadline {
                return Err(CredentialServiceStatus::TimedOut);
            }
            let mut buffer = [0; 4097];
            match stdout.read(&mut buffer) {
                Ok(0) => ended = true,
                Ok(count) => {
                    if bytes.len() + count > 4096 {
                        return Err(CredentialServiceStatus::Unknown);
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(CredentialServiceStatus::Unknown),
            }
            match child
                .try_wait()
                .map_err(|_| CredentialServiceStatus::Unknown)?
            {
                Some(status) if !status.success() => {
                    return Err(CredentialServiceStatus::Unavailable);
                }
                Some(_) if ended => return Ok(bytes),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    })();
    // No reader thread can outlive this deadline, including a full output pipe.
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    pub(super) fn fixture_command(executable: &Path) -> Command {
        if executable.file_name() == Some(std::ffi::OsStr::new("busctl-shim")) {
            // Read test source through a fixed interpreter rather than executing
            // a newly written text file. A concurrent inherited writer can make
            // exec return ETXTBSY even after the creating thread closed its fd.
            let mut command = Command::new("/usr/bin/python3");
            command.arg(executable);
            command
        } else {
            Command::new(executable)
        }
    }

    #[test]
    fn directory_diagnostic_is_read_only_and_distinguishes_missing_unsafe_ready() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let missing = temp.path().join("not-initialized/nested");
        assert_eq!(
            inspect_private_directory(&missing),
            StorageStatus::NotInitialized
        );
        assert!(!temp.path().join("not-initialized").exists());
        assert_eq!(inspect_private_directory(temp.path()), StorageStatus::Ready);
        let link = temp.path().join("linked-parent");
        symlink(temp.path(), &link).unwrap();
        assert_eq!(
            inspect_private_directory(&link.join("missing")),
            StorageStatus::Unsafe
        );
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            inspect_private_directory(temp.path()),
            StorageStatus::Unsafe
        );
        assert_eq!(
            inspect_private_directory(Path::new("relative")),
            StorageStatus::Unsafe
        );
    }

    #[test]
    fn directory_owner_modes_files_and_lexical_boundaries_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let uid = rustix::process::geteuid().as_raw();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            inspect_directory_owned_by(temp.path(), uid + 1),
            StorageStatus::Unsafe
        );
        for mode in [0o740, 0o755, 0o1700, 0o2700] {
            std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(
                inspect_private_directory(temp.path()),
                StorageStatus::Unsafe
            );
        }
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = temp.path().join("file");
        std::fs::write(&file, b"synthetic-do-not-read@example.test").unwrap();
        assert_eq!(inspect_private_directory(&file), StorageStatus::Unsafe);
        assert_eq!(
            inspect_private_directory(&temp.path().join("missing/../other")),
            StorageStatus::Unsafe
        );
        let link = temp.path().join("broken");
        symlink(temp.path().join("absent"), &link).unwrap();
        assert_eq!(inspect_private_directory(&link), StorageStatus::Unsafe);
        assert_eq!(
            std::fs::read(&file).unwrap(),
            b"synthetic-do-not-read@example.test"
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn bus_addresses_and_reply_shapes_reject_nonlocal_or_ambiguous_inputs() {
        for address in [
            "unix:path=/run/user/1000/bus",
            "unix:abstract=session-bus",
            "unix:path=/tmp/bus,guid=0123456789abcdef0123456789abcdef",
        ] {
            assert!(valid_local_bus_address(address));
        }
        for address in [
            "tcp:host=example.test",
            "unixexec:path=/bin/sh",
            "autolaunch:",
            "unix:path=relative",
            "unix:path=/tmp/bus;tcp:host=example.test",
            "unix:path=/tmp/bus,guid=short",
            "unix:path=/tmp/bus,exec=/bin/sh",
            "unix:abstract=",
            "unix:path=/tmp/a\n",
        ] {
            assert!(!valid_local_bus_address(address));
        }
        assert_eq!(
            collection_path(br#"{"type":"o","data":["/"]}"#).unwrap(),
            None
        );
        assert_eq!(
            collection_path(br#"{"type":"o","data":["/org/freedesktop/secrets/collection/test"]}"#)
                .unwrap()
                .as_deref(),
            Some("/org/freedesktop/secrets/collection/test")
        );
        for value in [
            br#"{"type":"o","data":["/outside"]}"#.as_slice(),
            br#"{"type":"o","data":["/org/freedesktop/secrets/collection/test//escape"]}"#,
            br#"{"type":"o","data":["/org/freedesktop/secrets/collection/test/.."]}"#,
            br#"{"type":"s","data":["/"]}"#,
            br#"{"type":"o","data":["/","/"]}"#,
            br#"{"type":"o","data":["/"],"extra":true}"#,
        ] {
            assert_eq!(
                collection_path(value),
                Err(CredentialServiceStatus::Unknown)
            );
        }
        for value in [
            br#"{"type":"b","data":false}"#.as_slice(),
            br#"{"type":"v","data":[{"type":"b","data":"false"}]}"#,
            br#"{"type":"v","data":[{"type":"b","data":false},{"type":"b","data":false}]}"#,
            br#"{"type":"v","data":[{"type":"s","data":false}]}"#,
            br#"{"type":"v","data":[{"type":"b","data":false,"extra":true}]}"#,
        ] {
            assert_eq!(locked_status(value), CredentialServiceStatus::Unknown);
        }
    }

    const SHIM: &str = r#"#!/usr/bin/python3
import json, os, sys, time
with open(sys.argv[0] + '.fixture') as source:
    fixture = json.load(source)
with open(sys.argv[0] + '.calls', 'a') as log:
    log.write(json.dumps({'args': sys.argv[1:], 'env': dict(os.environ)}) + '\n')
flags = ['--user', '--auto-start=no', '--allow-interactive-authorization=no', '--json=short', '--no-pager', '--timeout=3s']
assert sys.argv[1:7] == flags
args = sys.argv[7:]
alias = args == ['call', 'org.freedesktop.secrets', '/org/freedesktop/secrets', 'org.freedesktop.Secret.Service', 'ReadAlias', 's', 'default']
assert alias or args == ['call', 'org.freedesktop.secrets', '/org/freedesktop/secrets/collection/test', 'org.freedesktop.DBus.Properties', 'Get', 'ss', 'org.freedesktop.Secret.Collection', 'Locked']
time.sleep(fixture.get('delay', 0))
mode = fixture.get('mode')
if mode == 'error':
    print('synthetic-private@example.test token=synthetic-secret', file=sys.stderr)
    sys.exit(1)
if mode == 'oversize':
    sys.stdout.write('x' * 100000)
elif mode == 'malformed':
    print('synthetic-private@example.test token=synthetic-secret')
else:
    print(json.dumps(fixture.get('alias', {'type': 'o', 'data': ['/org/freedesktop/secrets/collection/test']}) if alias else {'type': 'v', 'data': [{'type': 'b', 'data': fixture.get('locked', False)}]}))
"#;

    fn run_fixture(
        fixture: serde_json::Value,
        budget: Duration,
    ) -> (CredentialServiceStatus, Vec<serde_json::Value>) {
        let temp = tempfile::tempdir().unwrap();
        let shim = temp.path().join("busctl-shim");
        std::fs::write(&shim, SHIM).unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            temp.path().join("busctl-shim.fixture"),
            serde_json::to_vec(&fixture).unwrap(),
        )
        .unwrap();
        std::fs::write(temp.path().join("busctl-shim.calls"), b"").unwrap();
        let _writer = fixture
            .get("hold_script_open")
            .and_then(serde_json::Value::as_bool)
            .filter(|hold| *hold)
            .map(|_| std::fs::OpenOptions::new().write(true).open(&shim).unwrap());
        let status = inspect_service_with(
            &shim,
            Some("unix:path=/synthetic/session-bus"),
            temp.path(),
            budget,
        );
        let calls = std::fs::read_to_string(temp.path().join("busctl-shim.calls"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect::<Vec<serde_json::Value>>();
        assert!(
            !calls.is_empty(),
            "synthetic metadata shim produced no call records; status={status:?}"
        );
        (status, calls)
    }

    #[test]
    fn metadata_fixture_can_be_read_while_a_script_writer_is_open() {
        let (status, calls) = run_fixture(
            serde_json::json!({"hold_script_open": true}),
            Duration::from_secs(2),
        );
        assert_eq!(status, CredentialServiceStatus::Unlocked);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn credential_metadata_uses_only_fixed_calls_and_cleared_environment() {
        use serde_json::json;
        for locked in [false, true] {
            let (status, calls) = run_fixture(json!({"locked": locked}), Duration::from_secs(2));
            assert_eq!(
                status,
                if locked {
                    CredentialServiceStatus::Locked
                } else {
                    CredentialServiceStatus::Unlocked
                }
            );
            assert_eq!(calls.len(), 2);
            for call in calls {
                let env = call["env"].as_object().unwrap();
                assert!(env.keys().all(|key| {
                    [
                        "XDG_RUNTIME_DIR",
                        "LANG",
                        "DBUS_SESSION_BUS_ADDRESS",
                        "LC_CTYPE",
                    ]
                    .contains(&key.as_str())
                }));
                assert_eq!(
                    env["DBUS_SESSION_BUS_ADDRESS"],
                    "unix:path=/synthetic/session-bus"
                );
                assert_eq!(call["args"][1], "--auto-start=no");
                assert_eq!(call["args"][2], "--allow-interactive-authorization=no");
            }
        }
        let (status, calls) = run_fixture(
            json!({"alias": {"type": "o", "data": ["/"]}}),
            Duration::from_secs(2),
        );
        assert_eq!(status, CredentialServiceStatus::NotInitialized);
        assert_eq!(calls.len(), 1);
        let (status, calls) = run_fixture(
            json!({"alias": {"type": "o", "data": ["/arbitrary/private"]}}),
            Duration::from_secs(2),
        );
        assert_eq!(status, CredentialServiceStatus::Unknown);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn credential_metadata_errors_output_floods_and_shared_deadline_are_bounded() {
        use serde_json::json;
        for (fixture, expected) in [
            (
                json!({"mode":"error"}),
                CredentialServiceStatus::Unavailable,
            ),
            (
                json!({"mode":"malformed"}),
                CredentialServiceStatus::Unknown,
            ),
            (json!({"mode":"oversize"}), CredentialServiceStatus::Unknown),
            (json!({"delay": 5}), CredentialServiceStatus::TimedOut),
            (json!({"delay": 0.12}), CredentialServiceStatus::TimedOut),
        ] {
            let started = Instant::now();
            let budget = if expected == CredentialServiceStatus::TimedOut {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(1)
            };
            let (status, _) = run_fixture(fixture, budget);
            assert_eq!(status, expected);
            assert!(started.elapsed() < Duration::from_secs(2));
            let output = serde_json::to_string(&status).unwrap();
            assert!(!output.contains("synthetic-secret"));
            assert!(!output.contains("@"));
        }
        assert_eq!(
            inspect_service_with(
                Path::new("/nonexistent-busctl"),
                None,
                Path::new("/synthetic"),
                Duration::from_secs(1)
            ),
            CredentialServiceStatus::Unavailable
        );
        assert_eq!(
            inspect_service_with(
                Path::new("/nonexistent-busctl"),
                Some("tcp:host=example.test"),
                Path::new("/synthetic"),
                Duration::from_secs(1)
            ),
            CredentialServiceStatus::Unknown
        );
    }
}
