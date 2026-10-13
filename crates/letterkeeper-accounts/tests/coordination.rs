// SPDX-License-Identifier: GPL-3.0-only
mod support;
use letterkeeper_accounts::*;
use std::{os::unix::fs::MetadataExt, path::PathBuf};

#[test]
fn mock_vault_coordination_defaults_to_none() {
    let fake = support::FakeStore::default();
    assert!(
        fake.coordination_directory(&support::config().id)
            .unwrap()
            .is_none()
    );
}
#[test]
fn system_vault_coordination_is_stable_private_and_does_not_create_directories() {
    let system = SystemCredentialStore::new();
    let uid = rustix::process::geteuid().as_raw();
    let runtime = PathBuf::from(format!("/run/user/{uid}"));
    let app = runtime.join("letterkeeper");
    let target = app.join("credential-locks/coordination-synthetic");
    let before = (
        app.exists(),
        app.join("credential-locks").exists(),
        target.exists(),
    );
    let first = system.coordination_directory("coordination-synthetic");
    let second = system.coordination_directory("coordination-synthetic");
    assert_eq!(first, second);
    let private = std::fs::symlink_metadata(&runtime)
        .is_ok_and(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o7777 == 0o700);
    if private {
        assert_eq!(first.unwrap().unwrap(), target);
    } else {
        assert_eq!(first.unwrap_err(), AccountError::UnsafeStorage);
    }
    assert!(
        before
            == (
                app.exists(),
                app.join("credential-locks").exists(),
                target.exists()
            )
    );
    for invalid in ["../escape", "Upper", "", "a/b"] {
        assert_eq!(
            system.coordination_directory(invalid).unwrap_err(),
            AccountError::InvalidConfiguration
        );
    }
}
#[test]
fn system_coordination_ignores_configuration_and_xdg_overrides() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "coordination_subprocess_probe", "--nocapture"])
        .env("LETTERKEEPER_COORDINATION_PROBE", "1")
        .env("HOME", "/tmp/synthetic-home")
        .env("XDG_CONFIG_HOME", "/tmp/synthetic-config")
        .env("XDG_DATA_HOME", "/tmp/synthetic-data")
        .env("XDG_RUNTIME_DIR", "/tmp/synthetic-runtime")
        .status()
        .unwrap();
    assert!(result.success());
}
#[test]
fn coordination_subprocess_probe() {
    if std::env::var("LETTERKEEPER_COORDINATION_PROBE").as_deref() != Ok("1") {
        return;
    }
    let system = SystemCredentialStore::new();
    let uid = rustix::process::geteuid().as_raw();
    let base = PathBuf::from(format!("/run/user/{uid}"));
    let result = system.coordination_directory("coordination-synthetic");
    let private = std::fs::symlink_metadata(&base)
        .is_ok_and(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o7777 == 0o700);
    if private {
        assert_eq!(
            result.unwrap().unwrap(),
            base.join("letterkeeper/credential-locks/coordination-synthetic")
        );
    } else {
        assert_eq!(result.unwrap_err(), AccountError::UnsafeStorage);
    }
}
