// SPDX-License-Identifier: GPL-3.0-only
#[path = "../../letterkeeper-protocol/tests/support.rs"]
mod support;
use letterkeeper::{remote::RemoteHost,sandbox::Sandbox};
use letterkeeper_accounts::*;
use letterkeeper_adapters::{Host,McpAdapter};
use letterkeeper_core::{AccessPolicy,Permissions,Store,SubmissionState};
use std::{path::PathBuf,sync::{Arc,atomic::{AtomicUsize,Ordering}}};
use serde_json::json;
#[derive(Default)]
struct Vault {gets:AtomicUsize}
impl CredentialStore for Vault {
    fn get(&self,_account:&str,_kind:SecretKind)->Result<Option<Secret>,AccountError>{self.gets.fetch_add(1,Ordering::SeqCst);Ok(Some(Secret::new("synthetic-secret".into())))}
    fn set(&self,_account:&str,_kind:SecretKind,_secret:&Secret)->Result<(),AccountError>{Ok(())}
    fn delete(&self,_account:&str,_kind:SecretKind)->Result<(),AccountError>{Ok(())}
}
fn config(imap:&support::Server,smtp:&support::Server)->AccountConfig{
    serde_json::from_value(json!({"id":"controlled","email":"reader@example.test","username":"person@example.test","imap":{"host":imap.endpoint.host,"port":imap.endpoint.port},"smtp":{"host":smtp.endpoint.host,"port":smtp.endpoint.port},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"append","authentication":{"type":"password"},"extra_ca_pem":null})).unwrap()
}
fn sandbox()->Sandbox{Sandbox{executable:PathBuf::from(env!("CARGO_BIN_EXE_letterkeeper")),launcher:"/usr/bin/bwrap".into()}}
fn host(cfg:AccountConfig,root:&std::path::Path,vault:Arc<Vault>,ca:&[u8])->RemoteHost {
    let cert=root.join("ca.pem");std::fs::write(&cert,ca).unwrap();let mut cfg=cfg;cfg.extra_ca_pem=Some(cert);
    RemoteHost::open(cfg,&root.join("state"),vault,sandbox()).unwrap()
}
fn policy()->AccessPolicy{AccessPolicy::new(vec!["controlled".into()],Permissions{read:true,draft:true,send:true})}
#[test]
fn controlled_host_reads_replies_and_syncs_without_marking_seen() {
    let imap=support::Server::start_shared();let smtp=support::Server::start(support::Scenario::Smtp);let cfg=config(&imap,&smtp);
    let dir=tempfile::tempdir().unwrap();let h=host(cfg,dir.path(),Arc::new(Vault::default()),imap.tls.roots_pem.as_ref().unwrap());
    let folders=h.list_folders().unwrap();assert_eq!(folders.len(),3);
    let messages=h.list_messages(0,20).unwrap();assert!(!messages.is_empty());
    let doc=h.read_message(&messages[0].id).unwrap();assert!(!doc.text.contains("<img"));assert!(!doc.text.contains("alert("));assert_eq!(doc.attachments.len(),1);
    let draft=h.reply_draft(&messages[0].id,"reply","合成回复",&[]).unwrap();assert_eq!(draft["to"][0],"replies@example.test");assert_eq!(draft["in_reply_to"],"<original@example.test>");
    let synced=h.sync_draft(&draft["id"].to_string()).unwrap();assert_eq!(synced["state"],"Confirmed");assert!(h.sync_draft(&draft["id"].to_string()).is_err());
    let lines=imap.finish();assert!(lines.iter().any(|s|s.contains("BODY.PEEK[]")));assert!(!lines.iter().any(|s|s.contains("UID STORE")));
}
#[test]
fn host_denies_scope_before_credentials_and_mcp_does_not_self_approve() {
    let imap=support::Server::start_shared();let smtp=support::Server::start(support::Scenario::Smtp);let mut cfg=config(&imap,&smtp);cfg.permissions.read=false;cfg.permissions.draft=false;cfg.permissions.send=false;
    let dir=tempfile::tempdir().unwrap();let vault=Arc::new(Vault::default());let h=host(cfg,dir.path(),vault.clone(),imap.tls.roots_pem.as_ref().unwrap());
    assert!(h.list_messages(0,20).is_err());assert!(h.read_message("r-bogus-77-11").is_err());assert!(h.send_draft("1").is_err());assert_eq!(vault.gets.load(Ordering::SeqCst),0);
    assert!(imap.finish().is_empty());
}
#[test]
fn smtp_uncertainty_persists_across_host_reopen_and_requires_no_blind_retry() {
    let imap=support::Server::start_shared();let smtp=support::Server::start(support::Scenario::SmtpDisconnect);let mut cfg=config(&imap,&smtp);
    let dir=tempfile::tempdir().unwrap();let ca=dir.path().join("ca.pem");std::fs::write(&ca,smtp.tls.roots_pem.as_ref().unwrap()).unwrap();cfg.extra_ca_pem=Some(ca);
    let vault=Arc::new(Vault::default());let state=dir.path().join("state");let h=RemoteHost::open(cfg.clone(),&state,vault.clone(),sandbox()).unwrap();
    let draft=h.create_draft(letterkeeper_adapters::DraftInput{to:vec!["to@example.test".into()],subject:"合成发送".into(),body:"test".into()}).unwrap();let id=draft["id"].as_i64().unwrap();
    assert!(h.send_draft(&id.to_string()).is_err());assert_eq!(vault.gets.load(Ordering::SeqCst),0);
    let mut s=Store::open(state.join("state.sqlite")).unwrap().with_policy(policy());let d=s.get_draft(id).unwrap();s.approve(id,&d.digest,i64::MAX).unwrap();drop(s);
    let sent=h.send_draft(&id.to_string()).unwrap();assert_eq!(sent["state"],"Uncertain");let sub=sent["id"].as_i64().unwrap();drop(h);
    let h=RemoteHost::open(cfg,&state,vault,sandbox()).unwrap();assert!(h.send_draft(&id.to_string()).is_err());assert!(h.sync_sent(&sub.to_string()).is_err());
    let s=Store::open(state.join("state.sqlite")).unwrap().with_policy(policy());assert_eq!(s.get_submission(sub).unwrap().state,SubmissionState::Uncertain);assert!(!s.submission_wire(sub).unwrap().is_empty());
    assert_eq!(smtp.finish().iter().filter(|s|s.starts_with("DATA")).count(),1);
}
#[test]
fn mcp_receives_inert_network_content_and_rejects_path_policy_inputs() {
    let imap=support::Server::start_shared();let smtp=support::Server::start(support::Scenario::Smtp);let cfg=config(&imap,&smtp);let dir=tempfile::tempdir().unwrap();
    let h=Arc::new(host(cfg,dir.path(),Arc::new(Vault::default()),imap.tls.roots_pem.as_ref().unwrap()));let adapter=McpAdapter::new(h);
    let rt=tokio::runtime::Runtime::new().unwrap();rt.block_on(async {
        let page=adapter.call("list_messages",json!({})).await.unwrap();let id=page["messages"][0]["id"].as_str().unwrap();
        let value=adapter.call("read_message",json!({"id":id})).await.unwrap();let text=value.to_string();assert!(!text.contains("http://"));assert!(!text.contains("<img"));assert!(!text.contains("auth=Bearer"));
        assert!(adapter.call("read_message",json!({"id":id,"path":"/tmp/private"})).await.is_err());
        assert!(adapter.call("create_draft",json!({"to":["to@example.test"],"subject":"s","body":"b","confirm":true})).await.is_err());
        let draft=adapter.call("reply_draft",json!({"id":id,"mode":"reply","body":"合成正文"})).await.unwrap();let draft_id=draft["id"].as_i64().unwrap();assert!(adapter.call("send_draft",json!({"id":draft_id.to_string()})).await.is_err());
    });
}
