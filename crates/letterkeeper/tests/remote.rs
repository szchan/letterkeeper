// SPDX-License-Identifier: GPL-3.0-only
#[path = "support/send_server.rs"]
mod send_server;
use letterkeeper::{remote::RemoteHost, sandbox::Sandbox};
use letterkeeper_accounts::*;
use letterkeeper_adapters::{Host, McpAdapter};
use letterkeeper_core::{AccessPolicy, Permissions, Store, SubmissionState};
use letterkeeper_test_support::mail as support;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
#[derive(Default)]
struct Vault {
    gets: AtomicUsize,
}
impl CredentialStore for Vault {
    fn get(&self, _account: &str, _kind: SecretKind) -> Result<Option<Secret>, AccountError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        Ok(Some(Secret::new("synthetic-secret".into())))
    }
    fn set(&self, _account: &str, _kind: SecretKind, _secret: &Secret) -> Result<(), AccountError> {
        Ok(())
    }
    fn delete(&self, _account: &str, _kind: SecretKind) -> Result<(), AccountError> {
        Ok(())
    }
}
fn config(imap: &support::Server, smtp: &support::Server) -> AccountConfig {
    serde_json::from_value(json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":imap.endpoint.host,"port":imap.endpoint.port},"smtp":{"host":smtp.endpoint.host,"port":smtp.endpoint.port},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"append","authentication":{"type":"password"},"extra_ca_pem":null})).unwrap()
}
fn sandbox() -> Sandbox {
    Sandbox {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_letterkeeper")),
        launcher: "/usr/bin/bwrap".into(),
    }
}
fn host(cfg: AccountConfig, root: &std::path::Path, vault: Arc<Vault>, ca: &[u8]) -> RemoteHost {
    let cert = root.join("ca.pem");
    std::fs::write(&cert, ca).unwrap();
    let mut cfg = cfg;
    cfg.extra_ca_pem = Some(cert);
    RemoteHost::open(cfg, &root.join("state"), vault, sandbox()).unwrap()
}
fn policy() -> AccessPolicy {
    AccessPolicy::new(
        vec!["controlled".into()],
        Permissions {
            read: true,
            draft: true,
            send: true,
        },
    )
}
#[test]
fn controlled_host_reads_replies_and_syncs_without_marking_seen() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let h = host(
        cfg,
        dir.path(),
        Arc::new(Vault::default()),
        imap.tls.roots_pem.as_ref().unwrap(),
    );
    let folders = h.list_folders().unwrap();
    assert_eq!(folders.len(), 3);
    let messages = h.list_messages(0, 20).unwrap();
    assert!(!messages.is_empty());
    let doc = h.read_message(&messages[0].id).unwrap();
    assert!(!doc.text.contains("<img"));
    assert!(!doc.text.contains("alert("));
    assert_eq!(doc.attachments.len(), 1);
    let draft = h
        .reply_draft(&messages[0].id, "reply", "合成回复", &[])
        .unwrap();
    assert_eq!(draft["to"][0], "reply@example.test");
    assert_eq!(draft["in_reply_to"], "<inbox@example.test>");
    let synced = h.sync_draft(&draft["id"].to_string()).unwrap();
    assert_eq!(synced["state"], "Confirmed");
    assert!(h.sync_draft(&draft["id"].to_string()).is_err());
    let lines = imap.finish();
    assert!(lines.iter().any(|s| s.contains("BODY.PEEK[]")));
    assert!(!lines.iter().any(|s| s.contains("UID STORE")));
}

#[test]
fn accepted_sent_copy_uses_exact_wire_once_and_evidence_is_not_delivery() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let mut cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let mut ca = imap.tls.roots_pem.clone().unwrap();
    ca.extend(smtp.tls.roots_pem.as_ref().unwrap());
    let cert = dir.path().join("ca.pem");
    std::fs::write(&cert, ca).unwrap();
    cfg.extra_ca_pem = Some(cert);
    let state = dir.path().join("state");
    let h = RemoteHost::open(cfg, &state, Arc::new(Vault::default()), sandbox()).unwrap();
    let d = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["to@example.test".into()],
            subject: "合成 MIME".into(),
            body: "test".into(),
        })
        .unwrap();
    let id = d["id"].as_i64().unwrap();
    let mut s = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    s.approve(id, d["digest"].as_str().unwrap(), i64::MAX)
        .unwrap();
    drop(s);
    let sent = h.send_draft(&id.to_string()).unwrap();
    assert_eq!(sent["state"], "Accepted");
    let sub = sent["id"].as_i64().unwrap();
    let copied = h.sync_sent(&sub.to_string()).unwrap();
    assert_eq!(copied["state"], "Confirmed");
    assert!(h.sync_sent(&sub.to_string()).is_err());
    let evidence = h.sent_evidence(&sub.to_string()).unwrap();
    assert_eq!(evidence["matches"].as_array().unwrap().len(), 1);
    assert_eq!(evidence["delivery_confirmed"], false);
    let s = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    let wire = s.submission_wire(sub).unwrap();
    let message_id = s.get_submission(sub).unwrap().message_id;
    let mut client_ca = imap.tls.roots_pem.clone().unwrap();
    client_ca.extend(smtp.tls.roots_pem.as_ref().unwrap());
    let client = letterkeeper_protocol::ImapClient::new(
        letterkeeper_protocol::Endpoint {
            host: imap.endpoint.host.clone(),
            port: imap.endpoint.port,
        },
        letterkeeper_protocol::TlsOptions {
            roots_pem: Some(client_ca),
        },
        letterkeeper_protocol::Credentials {
            username: "person@example.test".into(),
            secret: "synthetic-secret".into(),
            oauth: false,
        },
    );
    let found = client.lookup_message_id("Sent", &message_id).unwrap();
    let stored = client
        .read_message("Sent", found[0].uid_validity, found[0].uid)
        .unwrap();
    assert_eq!(stored, wire);
    let transcript = imap.finish();
    assert_eq!(
        transcript
            .iter()
            .filter(|line| line.contains(" APPEND "))
            .count(),
        1
    );
    assert_eq!(
        smtp.finish()
            .iter()
            .filter(|line| line.starts_with("DATA"))
            .count(),
        1
    );
}
#[test]
fn sent_evidence_respects_reduced_read_scope_before_vault_access() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let mut cfg = config(&imap, &smtp);
    cfg.allowed_folders = vec!["INBOX".into()];
    cfg.permissions.draft = false;
    cfg.permissions.send = false;
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let mut full_cfg = cfg.clone();
    full_cfg.allowed_folders = vec!["INBOX".into(), "Drafts".into(), "Sent".into()];
    full_cfg.permissions.draft = true;
    full_cfg.permissions.send = true;
    let mut s = letterkeeper::remote::store_for_account(&full_cfg, &state).unwrap();
    let input=serde_json::from_value(json!({"account":"controlled","from":"reader@example.test","to":["to@example.test"],"cc":[],"bcc":[],"subject":"test","body":"synthetic","attachments":[]})).unwrap();
    let d = s.create_draft(input).unwrap();
    s.approve(d.id, &d.digest, i64::MAX).unwrap();
    let sub = s.claim_send(d.id, 1).unwrap();
    drop(s);
    let vault = Arc::new(Vault::default());
    let h = RemoteHost::open(cfg, &state, vault.clone(), sandbox()).unwrap();
    assert!(h.sent_evidence(&sub.id.to_string()).is_err());
    assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
    assert!(imap.finish().is_empty());
}
#[test]
fn host_denies_scope_before_credentials_and_mcp_does_not_self_approve() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let mut cfg = config(&imap, &smtp);
    cfg.permissions.read = false;
    cfg.permissions.draft = false;
    cfg.permissions.send = false;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::default());
    let h = host(
        cfg,
        dir.path(),
        vault.clone(),
        imap.tls.roots_pem.as_ref().unwrap(),
    );
    assert!(h.list_messages(0, 20).is_err());
    assert!(h.read_message("r-bogus-77-11").is_err());
    assert!(h.send_draft("1").is_err());
    assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
    assert!(imap.finish().is_empty());
}
#[test]
fn smtp_uncertainty_persists_across_host_reopen_and_requires_no_blind_retry() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::SmtpDisconnect);
    let mut cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, smtp.tls.roots_pem.as_ref().unwrap()).unwrap();
    cfg.extra_ca_pem = Some(ca);
    let vault = Arc::new(Vault::default());
    let state = dir.path().join("state");
    let h = RemoteHost::open(cfg.clone(), &state, vault.clone(), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["to@example.test".into()],
            subject: "合成发送".into(),
            body: "test".into(),
        })
        .unwrap();
    let id = draft["id"].as_i64().unwrap();
    assert!(h.send_draft(&id.to_string()).is_err());
    assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
    let mut s = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    let d = s.get_draft(id).unwrap();
    s.approve(id, &d.digest, i64::MAX).unwrap();
    drop(s);
    let sent = h.send_draft(&id.to_string()).unwrap();
    assert_eq!(sent["state"], "Uncertain");
    let sub = sent["id"].as_i64().unwrap();
    drop(h);
    let h = RemoteHost::open(cfg, &state, vault, sandbox()).unwrap();
    assert!(h.send_draft(&id.to_string()).is_err());
    assert!(h.sync_sent(&sub.to_string()).is_err());
    let s = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    assert_eq!(
        s.get_submission(sub).unwrap().state,
        SubmissionState::Uncertain
    );
    assert!(!s.submission_wire(sub).unwrap().is_empty());
    assert_eq!(
        smtp.finish()
            .iter()
            .filter(|s| s.starts_with("DATA"))
            .count(),
        1
    );
}
#[test]
fn mcp_receives_inert_network_content_and_rejects_path_policy_inputs() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let h = Arc::new(host(
        cfg,
        dir.path(),
        Arc::new(Vault::default()),
        imap.tls.roots_pem.as_ref().unwrap(),
    ));
    let adapter = McpAdapter::new(h);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let page = adapter.call("list_messages", json!({})).await.unwrap();
        let id = page["messages"][0]["id"].as_str().unwrap();
        let value = adapter
            .call("read_message", json!({"id":id}))
            .await
            .unwrap();
        let text = value.to_string();
        assert!(!text.contains("http://"));
        assert!(!text.contains("<img"));
        assert!(!text.contains("auth=Bearer"));
        assert!(
            adapter
                .call("read_message", json!({"id":id,"path":"/tmp/private"}))
                .await
                .is_err()
        );
        assert!(
            adapter
                .call(
                    "create_draft",
                    json!({"to":["to@example.test"],"subject":"s","body":"b","confirm":true})
                )
                .await
                .is_err()
        );
        let draft = adapter
            .call(
                "reply_draft",
                json!({"id":id,"mode":"reply","body":"合成正文"}),
            )
            .await
            .unwrap();
        let draft_id = draft["id"].as_i64().unwrap();
        assert!(
            adapter
                .call("send_draft", json!({"id":draft_id.to_string()}))
                .await
                .is_err()
        );
    });
}

#[test]
fn missing_credentials_do_not_consume_a_draft_sync_revision() {
    struct OptionalVault(std::sync::atomic::AtomicBool);
    impl CredentialStore for OptionalVault {
        fn get(&self, _account: &str, _kind: SecretKind) -> Result<Option<Secret>, AccountError> {
            Ok(self
                .0
                .load(Ordering::SeqCst)
                .then(|| Secret::new("synthetic-secret".into())))
        }
        fn set(
            &self,
            _account: &str,
            _kind: SecretKind,
            _secret: &Secret,
        ) -> Result<(), AccountError> {
            Ok(())
        }
        fn delete(&self, _account: &str, _kind: SecretKind) -> Result<(), AccountError> {
            Ok(())
        }
    }
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let mut cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, imap.tls.roots_pem.as_ref().unwrap()).unwrap();
    cfg.extra_ca_pem = Some(ca);
    let vault = Arc::new(OptionalVault(std::sync::atomic::AtomicBool::new(false)));
    let h = RemoteHost::open(cfg, &dir.path().join("state"), vault.clone(), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["to@example.test".into()],
            subject: "test".into(),
            body: "synthetic".into(),
        })
        .unwrap();
    let id = draft["id"].to_string();
    assert!(h.sync_draft(&id).is_err());
    vault.0.store(true, Ordering::SeqCst);
    assert_eq!(h.sync_draft(&id).unwrap()["state"], "Confirmed");
}

#[test]
fn server_managed_sent_policy_never_issues_client_append() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let mut cfg = config(&imap, &smtp);
    cfg.sent_copy = SentCopyStrategy::ServerManaged;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(Vault::default());
    let h = host(
        cfg,
        dir.path(),
        vault.clone(),
        imap.tls.roots_pem.as_ref().unwrap(),
    );
    assert!(h.sync_sent("1").is_err());
    assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
    assert!(imap.finish().is_empty());
}

#[test]
fn failed_send_has_safe_persistent_diagnostic() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::SmtpReject);
    let mut cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, smtp.tls.roots_pem.as_ref().unwrap()).unwrap();
    cfg.extra_ca_pem = Some(ca);
    let state = dir.path().join("state");
    let vault = Arc::new(Vault::default());
    let h = RemoteHost::open(cfg.clone(), &state, vault.clone(), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["private-recipient@example.test".into()],
            subject: "合成诊断".into(),
            body: "private synthetic body".into(),
        })
        .unwrap();
    let id = draft["id"].as_i64().unwrap();
    assert!(h.send_draft(&id.to_string()).is_err());
    assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
    let mut store = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    store
        .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
        .unwrap();
    drop(store);
    let sent = h.send_draft(&id.to_string()).unwrap();
    assert_eq!(sent["state"], "Failed");
    assert_eq!(
        sent["diagnostic"],
        json!({"stage":"recipient","kind":"rejected","smtp_code":550})
    );
    let submission = sent["id"].as_i64().unwrap();
    drop(h);
    let reopened = RemoteHost::open(cfg, &state, vault, sandbox()).unwrap();
    let status = reopened.send_status(&submission.to_string()).unwrap();
    assert_eq!(status["diagnostic"], sent["diagnostic"]);
    for forbidden in [
        "private-recipient@example.test",
        "person@example.test",
        "synthetic-secret",
        "private synthetic body",
        "refused",
    ] {
        assert!(!sent.to_string().contains(forbidden));
        assert!(!status.to_string().contains(forbidden));
    }
    assert!(smtp.finish().iter().any(|line| line.starts_with("RCPT")));
}

#[test]
fn tls_preflight_failure_never_reads_credentials() {
    for ca_bytes in [None, Some(b"invalid PEM synthetic-secret".as_slice())] {
        let imap = support::Server::start_shared();
        let smtp = support::Server::start(support::Scenario::Smtp);
        let mut cfg = config(&imap, &smtp);
        let dir = tempfile::tempdir().unwrap();
        let ca = dir.path().join("bad.pem");
        if let Some(bytes) = ca_bytes {
            std::fs::write(&ca, bytes).unwrap();
        }
        cfg.extra_ca_pem = Some(ca);
        let state = dir.path().join("state");
        let vault = Arc::new(Vault::default());
        let h = RemoteHost::open(cfg, &state, vault.clone(), sandbox()).unwrap();
        let draft = h
            .create_draft(letterkeeper_adapters::DraftInput {
                to: vec!["to@example.test".into()],
                subject: "test".into(),
                body: "test".into(),
            })
            .unwrap();
        let id = draft["id"].as_i64().unwrap();
        let mut store = Store::open(state.join("state.sqlite"))
            .unwrap()
            .with_policy(policy());
        store
            .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
            .unwrap();
        drop(store);
        let sent = h.send_draft(&id.to_string()).unwrap();
        assert_eq!(sent["state"], "Failed");
        assert_eq!(sent["diagnostic"]["stage"], "tls_configuration");
        assert_eq!(
            sent["diagnostic"]["kind"],
            if ca_bytes.is_some() {
                "invalid_input"
            } else {
                "unavailable"
            }
        );
        assert_eq!(vault.gets.load(Ordering::SeqCst), 0);
        assert!(smtp.finish().is_empty());
    }
}
struct UnavailableVault;
impl CredentialStore for UnavailableVault {
    fn get(&self, _: &str, _: SecretKind) -> Result<Option<Secret>, AccountError> {
        Ok(None)
    }
    fn set(&self, _: &str, _: SecretKind, _: &Secret) -> Result<(), AccountError> {
        unreachable!()
    }
    fn delete(&self, _: &str, _: SecretKind) -> Result<(), AccountError> {
        unreachable!()
    }
}
#[test]
fn unavailable_credentials_are_diagnostic_and_do_not_connect() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let h = RemoteHost::open(cfg, &state, Arc::new(UnavailableVault), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["to@example.test".into()],
            subject: "test".into(),
            body: "test".into(),
        })
        .unwrap();
    let mut store = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    let id = draft["id"].as_i64().unwrap();
    store
        .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
        .unwrap();
    drop(store);
    let sent = h.send_draft(&id.to_string()).unwrap();
    assert_eq!(sent["state"], "Failed");
    assert_eq!(
        sent["diagnostic"],
        json!({"stage":"credentials","kind":"unavailable","smtp_code":null})
    );
    assert!(smtp.finish().is_empty());
}
#[test]
fn cli_status_matches_remote_diagnostic_without_sensitive_reply_text() {
    for mode in [
        send_server::ResultMode::Accepted,
        send_server::ResultMode::Rejected,
        send_server::ResultMode::Disconnected,
    ] {
        let smtp = send_server::Server::start(mode);
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let data_dir = dir.path().join("data");
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, &smtp.pem).unwrap();
        let cfg: AccountConfig = serde_json::from_value(json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":"localhost","port":9993},"smtp":{"host":"localhost","port":smtp.port},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed","authentication":{"type":"password"},"extra_ca_pem":ca})).unwrap();
        AccountRepository::new(config_dir.clone(), data_dir.clone())
            .unwrap()
            .save(&cfg)
            .unwrap();
        let state = data_dir.join("controlled");
        let h = RemoteHost::open(cfg, &state, Arc::new(Vault::default()), sandbox()).unwrap();
        let draft = h
            .create_draft(letterkeeper_adapters::DraftInput {
                to: vec!["recipient@example.test".into()],
                subject: "合成提交".into(),
                body: "synthetic body".into(),
            })
            .unwrap();
        let id = draft["id"].as_i64().unwrap();
        let mut store = Store::open(state.join("state.sqlite"))
            .unwrap()
            .with_policy(policy());
        store
            .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
            .unwrap();
        drop(store);
        let sent = h.send_draft(&id.to_string()).unwrap();
        assert!(h.send_draft(&id.to_string()).is_err());
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
            .arg("--config-dir")
            .arg(&config_dir)
            .arg("--data-dir")
            .arg(&data_dir)
            .args(["--account", "controlled", "status"])
            .arg(sent["id"].to_string())
            .output()
            .unwrap();
        assert!(output.status.success());
        let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(status["state"], sent["state"]);
        assert_eq!(status["diagnostic"], sent["diagnostic"]);
        match mode {
            send_server::ResultMode::Accepted => {
                assert_eq!(sent["state"], "Accepted");
                assert!(sent.get("diagnostic").unwrap().is_null());
            }
            send_server::ResultMode::Rejected => {
                assert_eq!(sent["state"], "Failed");
                assert_eq!(
                    sent["diagnostic"],
                    json!({"stage":"submission","kind":"rejected","smtp_code":550})
                );
            }
            send_server::ResultMode::Disconnected => {
                assert_eq!(sent["state"], "Uncertain");
                assert_eq!(sent["diagnostic"]["stage"], "submission");
            }
        }
        for value in [&sent, &status] {
            for forbidden in [
                send_server::PRIVATE_REPLY,
                "recipient@example.test",
                "synthetic-secret",
                "synthetic body",
            ] {
                assert!(!value.to_string().contains(forbidden));
            }
        }
        assert_eq!(
            smtp.finish()
                .iter()
                .filter(|line| line.starts_with("DATA"))
                .count(),
            1
        );
    }
}

#[test]
fn submissions_finished_without_diagnostics_remain_explicitly_unknown() {
    let imap = support::Server::start_shared();
    let smtp = support::Server::start(support::Scenario::Smtp);
    let cfg = config(&imap, &smtp);
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    let h = RemoteHost::open(cfg, &state, Arc::new(Vault::default()), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["to@example.test".into()],
            subject: "legacy".into(),
            body: "test".into(),
        })
        .unwrap();
    let id = draft["id"].as_i64().unwrap();
    let mut store = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    store
        .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
        .unwrap();
    let sub = store.claim_send(id, 1).unwrap();
    store
        .finish_submission(sub.id, letterkeeper_core::SendOutcome::Failed)
        .unwrap();
    drop(store);
    let status = h.send_status(&sub.id.to_string()).unwrap();
    assert_eq!(status["state"], "Failed");
    assert!(status.get("diagnostic").unwrap().is_null());
    assert!(smtp.finish().is_empty());
}

#[test]
fn diagnostic_query_failure_keeps_known_host_and_cli_status() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("config");
    let data_dir = dir.path().join("data");
    let cfg: AccountConfig = serde_json::from_value(json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":"localhost","port":9993},"smtp":{"host":"localhost","port":9465},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed","authentication":{"type":"password"},"extra_ca_pem":null})).unwrap();
    AccountRepository::new(config_dir.clone(), data_dir.clone())
        .unwrap()
        .save(&cfg)
        .unwrap();
    let state = data_dir.join("controlled");
    let h = RemoteHost::open(cfg, &state, Arc::new(Vault::default()), sandbox()).unwrap();
    let draft = h
        .create_draft(letterkeeper_adapters::DraftInput {
            to: vec!["recipient@example.test".into()],
            subject: "synthetic".into(),
            body: "synthetic".into(),
        })
        .unwrap();
    let mut store = Store::open(state.join("state.sqlite"))
        .unwrap()
        .with_policy(policy());
    let id = draft["id"].as_i64().unwrap();
    store
        .approve(id, draft["digest"].as_str().unwrap(), i64::MAX)
        .unwrap();
    let sub = store.claim_send(id, 1).unwrap();
    store
        .finish_submission(sub.id, letterkeeper_core::SendOutcome::Accepted)
        .unwrap();
    drop(store);
    // Corrupt only diagnostic availability after initialization; submission remains readable.
    let changed = std::process::Command::new("python3")
        .args(["-c", "import sqlite3,sys; db=sqlite3.connect(sys.argv[1]); db.execute('DROP TABLE submission_diagnostics'); db.commit(); db.close()"])
        .arg(state.join("state.sqlite")).status().unwrap();
    assert!(changed.success());
    let status = h.send_status(&sub.id.to_string());
    assert!(
        status.is_ok(),
        "known submission state must survive diagnostic query failure"
    );
    let status = status.unwrap();
    assert_eq!(status["state"], "Accepted");
    assert_eq!(status["state_recorded"], true);
    assert!(status.get("diagnostic").unwrap().is_null());
    assert_eq!(status["diagnostic_query_succeeded"], false);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_letterkeeper"))
        .arg("--config-dir")
        .arg(config_dir)
        .arg("--data-dir")
        .arg(data_dir)
        .args(["--account", "controlled", "status"])
        .arg(sub.id.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI must retain readable submission state"
    );
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for field in [
        "state",
        "state_recorded",
        "diagnostic",
        "diagnostic_query_succeeded",
    ] {
        assert_eq!(cli[field], status[field]);
    }
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        for forbidden in [
            "no such table",
            "submission_diagnostics",
            "recipient@example.test",
            "synthetic-secret",
        ] {
            assert!(!text.contains(forbidden));
        }
    }
    assert!(
        h.send_status("999999").is_err(),
        "missing submission still fails"
    );
}
