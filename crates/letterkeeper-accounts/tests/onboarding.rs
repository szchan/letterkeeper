// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_accounts::*;
use std::{
    os::unix::fs::{PermissionsExt, symlink},
    sync::{Arc, Barrier},
};

fn input() -> OnboardingInput {
    OnboardingInput {
        id: "synthetic".into(),
        email: "reader@example.test".into(),
        username: None,
        allowed_folders: vec!["INBOX".into(), "项目邮件".into()],
        scope: OnboardingScope::default(),
        drafts_folder: None,
        sent_folder: None,
        provider: OnboardingProvider::Password {
            imap: OnboardingEndpoint {
                host: "imap.example.test".into(),
                port: None,
            },
            smtp: OnboardingEndpoint {
                host: "smtp.example.test".into(),
                port: None,
            },
            sent_copy: SentCopyStrategy::Append,
        },
    }
}

#[test]
fn onboarding_rejects_bidi_format_marks_in_reviewed_account_fields() {
    for mark in ['\u{061c}', '\u{200e}', '\u{200f}'] {
        let mut i = input();
        i.allowed_folders.push(format!("项目{mark}"));
        assert!(i.build().is_err(), "bidi format mark accepted in folder");
        let mut i = input();
        i.username = Some(format!("synthetic{mark}"));
        assert!(i.build().is_err(), "bidi format mark accepted in username");
        let mut i = input();
        i.drafts_folder = Some(format!("草稿{mark}"));
        assert!(i.build().is_err());
        let mut i = input();
        i.sent_folder = Some(format!("已发送{mark}"));
        assert!(i.build().is_err());
    }
}

#[test]
fn onboarding_defaults_and_create_only_contract() {
    let c = input()
        .build()
        .expect("offline account setup should validate");
    assert_eq!(c.imap.port, 993);
    assert_eq!(c.smtp.port, 465);
    assert_eq!(c.username, c.email);
    assert_eq!(c.allowed_folders, ["INBOX", "项目邮件"]);
    assert_eq!(
        c.permissions,
        OperationPermissions {
            read: true,
            draft: false,
            send: false
        }
    );
    assert_eq!(c.authentication, Authentication::Password);
    assert!(c.extra_ca_pem.is_none());
    let t = tempfile::tempdir().unwrap();
    let repo = AccountRepository::new(t.path().join("config"), t.path().join("data")).unwrap();
    repo.create(&c).unwrap();
    let path = repo.config_dir().join("synthetic.json");
    let original = std::fs::read(&path).unwrap();
    let mut changed = c.clone();
    changed.email = "other@example.test".into();
    assert_eq!(
        repo.create(&changed),
        Err(AccountError::AccountAlreadyExists)
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(repo.load("synthetic").unwrap(), c);
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(std::fs::read_dir(repo.config_dir()).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(repo.data_dir()).unwrap().count(), 0);
}

#[test]
fn onboarding_gmail_is_fixed_and_scope_additions_are_explicit() {
    for (scope, draft, send) in [
        (OnboardingScope::ReadOnly, false, false),
        (OnboardingScope::Draft, true, false),
        (OnboardingScope::Send, true, true),
    ] {
        let mut i = input();
        i.provider = OnboardingProvider::Gmail {
            client_id: "synthetic.apps.googleusercontent.com".into(),
            client_secret_ref: true,
        };
        i.scope = scope;
        let c = i.build().unwrap();
        assert_eq!(
            c.imap,
            TlsEndpoint {
                host: "imap.gmail.com".into(),
                port: 993
            }
        );
        assert_eq!(
            c.smtp,
            TlsEndpoint {
                host: "smtp.gmail.com".into(),
                port: 465
            }
        );
        assert_eq!(c.sent_copy, SentCopyStrategy::ServerManaged);
        assert_eq!(
            c.permissions,
            OperationPermissions {
                read: true,
                draft,
                send
            }
        );
        assert_eq!(c.allowed_folders.contains(&c.drafts_folder), draft);
        assert_eq!(c.allowed_folders.contains(&c.sent_folder), send);
        assert_eq!(c.drafts_folder, "[Gmail]/Drafts");
        assert_eq!(c.sent_folder, "[Gmail]/Sent Mail");
        assert!(matches!(
            c.authentication,
            Authentication::GmailOAuth {
                client_secret_ref: true,
                ..
            }
        ));
    }
    let mut i = input();
    i.scope = OnboardingScope::Send;
    i.username = Some("synthetic-login".into());
    i.drafts_folder = Some("草稿".into());
    i.sent_folder = Some("已发送".into());
    let c = i.build().unwrap();
    assert_eq!(c.username, "synthetic-login");
    assert_eq!(c.allowed_folders, ["INBOX", "项目邮件", "草稿", "已发送"]);
}

#[test]
fn onboarding_rejects_invalid_untrusted_and_oversized_inputs() {
    for id in ["", "Upper", "../escape", "a\nb", &"a".repeat(65)] {
        let mut i = input();
        i.id = id.into();
        assert!(i.build().is_err());
    }
    for email in [
        "",
        "missing-at",
        "a@@example.test",
        ".a@example.test",
        "a..b@example.test",
        "a@-example.test",
        "a@example..test",
        "a@bad_underscore.test",
        "a@example.test\r\n",
        "a @example.test",
    ] {
        let mut i = input();
        i.email = email.into();
        assert!(i.build().is_err(), "email accepted");
    }
    for host in [
        "",
        "https://imap.example.test",
        "host:993",
        ".",
        "-host.test",
        "host-.test",
        "a..b",
        "a.b_underscore",
        "a\u{202e}.test",
        &"x".repeat(254),
    ] {
        let mut i = input();
        if let OnboardingProvider::Password { imap, .. } = &mut i.provider {
            imap.host = host.into();
        }
        assert!(i.build().is_err(), "host accepted");
    }
    let mut i = input();
    if let OnboardingProvider::Password { smtp, .. } = &mut i.provider {
        smtp.port = Some(0);
    }
    assert!(i.build().is_err());
    for folders in [
        vec![],
        vec!["INBOX".into(), "INBOX".into()],
        vec![" ".into()],
        vec!["evil\u{001b}[2J".into()],
        vec!["x".repeat(513)],
        vec!["x".into(); 129],
    ] {
        let mut i = input();
        i.allowed_folders = folders;
        assert!(i.build().is_err());
    }
    for value in ["", " ", "client id", "evil\n", &"x".repeat(513)] {
        let mut i = input();
        i.provider = OnboardingProvider::Gmail {
            client_id: value.into(),
            client_secret_ref: false,
        };
        assert!(i.build().is_err());
    }
    for value in ["", " ", "evil\u{2066}", &"x".repeat(513)] {
        let mut i = input();
        i.drafts_folder = Some(value.into());
        assert!(i.build().is_err());
    }
    let mut i = input();
    i.username = Some(" ".into());
    assert!(i.build().is_err());
}

#[test]
fn onboarding_bounds_and_nonduplicating_scope_folders() {
    let mut i = input();
    i.scope = OnboardingScope::Send;
    i.allowed_folders = vec!["INBOX".into(), "Drafts".into(), "Sent".into()];
    let c = i.build().unwrap();
    assert_eq!(c.allowed_folders.len(), 3);
    let mut i = input();
    i.scope = OnboardingScope::Send;
    i.allowed_folders = (0..128).map(|n| format!("folder-{n}")).collect();
    assert!(i.build().is_err());
    let mut i = input();
    if let OnboardingProvider::Password { imap, smtp, .. } = &mut i.provider {
        imap.host = "::1".into();
        imap.port = Some(1993);
        smtp.port = Some(1465);
    }
    let c = i.build().unwrap();
    assert_eq!(c.imap.port, 1993);
    assert_eq!(c.smtp.port, 1465);
}

#[test]
fn create_only_rejects_every_existing_type_and_racing_creators() {
    let t = tempfile::tempdir().unwrap();
    let repo = AccountRepository::new(t.path().join("config"), t.path().join("data")).unwrap();
    let c = input().build().unwrap();
    let path = repo.config_dir().join("synthetic.json");
    std::fs::create_dir(&path).unwrap();
    assert_eq!(repo.create(&c), Err(AccountError::AccountAlreadyExists));
    std::fs::remove_dir(&path).unwrap();
    symlink(t.path().join("missing-target"), &path).unwrap();
    assert_eq!(repo.create(&c), Err(AccountError::AccountAlreadyExists));
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    std::fs::remove_file(&path).unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let barrier = barrier.clone();
                let repo = &repo;
                let mut c = c.clone();
                c.username = format!("synthetic-{n}");
                scope.spawn(move || {
                    barrier.wait();
                    repo.create(&c)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(AccountError::AccountAlreadyExists))
            .count(),
        7
    );
    assert_eq!(std::fs::read_dir(repo.config_dir()).unwrap().count(), 1);
    repo.load("synthetic").unwrap();
}
