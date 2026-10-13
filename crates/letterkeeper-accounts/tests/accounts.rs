// SPDX-License-Identifier: GPL-3.0-only
mod support;
use letterkeeper_accounts::*;
use std::os::unix::fs::{PermissionsExt, symlink};
use support::*;
#[test]
fn strict_configuration_namespace_scope_and_public_serialization() {
    let c = config();
    c.validate().unwrap();
    let mut value = serde_json::to_value(&c).unwrap();
    value["password"] = "SYNTHETIC_SECRET".into();
    assert!(serde_json::from_value::<AccountConfig>(value).is_err());
    for id in ["../escape", "", "a/b", "x\ny", "Upper", "a b"] {
        let mut c = config();
        c.id = id.into();
        assert!(c.validate().is_err());
    }
    let mut c = config();
    c.allowed_folders = vec!["INBOX".into()];
    assert!(c.validate().is_err());
    let mut c = config();
    c.smtp.port = 0;
    assert!(c.validate().is_err());
    let mut c = config();
    c.imap.host = "bad\r\n.example.test".into();
    assert!(c.validate().is_err());
    let mut c = config();
    c.email = "bad@example.test\r\n".into();
    assert!(c.validate().is_err());
}
#[test]
fn secrets_redact_and_password_uses_account_namespace() {
    let s = Secret::new("SYNTHETIC_SECRET".into());
    assert!(!format!("{s:?}").contains("SYNTHETIC_SECRET"));
    let store = FakeStore::default();
    assert!(password_auth_material(&config(), &store).is_err());
    store.set("other", SecretKind::Password, &s).unwrap();
    assert!(password_auth_material(&config(), &store).is_err());
    store.set("synthetic", SecretKind::Password, &s).unwrap();
    let a = password_auth_material(&config(), &store).unwrap();
    assert!(!a.oauth);
    assert_eq!(a.secret.expose(), "SYNTHETIC_SECRET");
}
#[test]
fn private_repository_atomic_roundtrip_and_rejects_links_permissions_bounds() {
    let temp = tempfile::tempdir().unwrap();
    let repo =
        AccountRepository::new(temp.path().join("config"), temp.path().join("data")).unwrap();
    repo.save(&config()).unwrap();
    assert_eq!(repo.load("synthetic").unwrap(), config());
    assert_eq!(repo.list().unwrap(), vec!["synthetic"]);
    assert_eq!(
        std::fs::metadata(repo.config_dir())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let p = repo.config_dir().join("synthetic.json");
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(repo.load("synthetic").is_err());
    assert!(repo.save(&config()).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::hard_link(&p, repo.config_dir().join("other.json")).unwrap();
    assert!(repo.load("synthetic").is_err());
    assert!(repo.save(&config()).is_err());
    std::fs::remove_file(repo.config_dir().join("other.json")).unwrap();
    std::fs::remove_file(&p).unwrap();
    symlink(temp.path().join("outside"), &p).unwrap();
    assert!(repo.save(&config()).is_err());
    std::fs::remove_file(&p).unwrap();
    std::fs::write(&p, vec![b'x'; MAX_CONFIG_BYTES + 1]).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(repo.load("synthetic").is_err());
    assert!(repo.load("../escape").is_err());
}
#[test]
fn repository_refuses_symlink_directory_and_public_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("public")).unwrap();
    assert!(AccountRepository::new(temp.path().join("public"), temp.path().join("data")).is_err());
    symlink(temp.path().join("public"), temp.path().join("link")).unwrap();
    assert!(AccountRepository::new(temp.path().join("link"), temp.path().join("data")).is_err());
}

#[test]
fn repository_replacement_is_atomic_and_id_is_bound_to_filename() {
    let t = tempfile::tempdir().unwrap();
    let repo = AccountRepository::new(t.path().join("config"), t.path().join("data")).unwrap();
    repo.save(&config()).unwrap();
    let mut replacement = config();
    replacement.permissions.send = false;
    repo.save(&replacement).unwrap();
    assert_eq!(repo.load("synthetic").unwrap(), replacement);
    assert_eq!(std::fs::read_dir(repo.config_dir()).unwrap().count(), 1);
    let p = repo.config_dir().join("other.json");
    std::fs::write(&p, serde_json::to_vec(&replacement).unwrap()).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        repo.load("other").unwrap_err(),
        AccountError::InvalidConfiguration
    );
}

#[test]
fn default_paths_follow_xdg_without_creating_storage() {
    for (config, data) in [
        ("/tmp/synthetic-config", "/tmp/synthetic-data"),
        ("relative-config", "relative-data"),
    ] {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "default_paths_subprocess_probe", "--nocapture"])
            .env("LETTERKEEPER_PATH_PROBE", "1")
            .env("HOME", "/tmp/synthetic-home")
            .env("XDG_CONFIG_HOME", config)
            .env("XDG_DATA_HOME", data)
            .status()
            .unwrap();
        assert!(result.success());
    }
}
#[test]
fn default_paths_subprocess_probe() {
    if std::env::var("LETTERKEEPER_PATH_PROBE").as_deref() != Ok("1") {
        return;
    }
    let (config, data) = AccountRepository::default_paths().unwrap();
    let explicit = std::env::var("XDG_CONFIG_HOME").unwrap().starts_with('/');
    assert_eq!(
        config,
        std::path::PathBuf::from(if explicit {
            "/tmp/synthetic-config/letterkeeper"
        } else {
            "/tmp/synthetic-home/.config/letterkeeper"
        })
    );
    assert_eq!(
        data,
        std::path::PathBuf::from(if explicit {
            "/tmp/synthetic-data/letterkeeper"
        } else {
            "/tmp/synthetic-home/.local/share/letterkeeper"
        })
    );
}
