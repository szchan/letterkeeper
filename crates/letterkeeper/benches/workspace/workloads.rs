// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic Rust workloads, deliberately outside the ordinary test harness.
use letterkeeper::{app::AppHost, remote::RemoteHost};
use letterkeeper_accounts::{AccountConfig, AccountError, CredentialStore, Secret, SecretKind};
use letterkeeper_adapters::{
    DraftInput as AgentDraft, Host, HostError, MessageSummary, run_tui_workspace,
    workspace::HumanWorkspace,
};
use letterkeeper_content::SafeDocument;
use letterkeeper_core::DraftInput;
use letterkeeper_test_support::performance as performance_server;
use serde_json::Value;
use std::sync::Arc;

pub fn child(name: &str) -> Result<(), String> {
    match name {
        "workspace_benchmark_child" => workspace_benchmark_child(),
        "workspace_tls_benchmark_child" => workspace_tls_benchmark_child(),
        "workspace_tls_mcp_benchmark_child" => workspace_tls_mcp_benchmark_child(),
        _ => return Err("unknown benchmark workload".into()),
    }
    Ok(())
}
struct BenchmarkHost {
    messages: Vec<MessageSummary>,
    body_bytes: usize,
}
impl Host for BenchmarkHost {
    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self
            .messages
            .iter()
            .skip(offset)
            .take(limit.min(50))
            .cloned()
            .collect())
    }
    fn search_messages(
        &self,
        _: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        Ok(self
            .messages
            .iter()
            .filter(|m| m.subject.contains(query))
            .skip(offset)
            .take(limit.min(50))
            .cloned()
            .collect())
    }
    fn read_message(&self, id: &str) -> Result<SafeDocument, HostError> {
        if !self.messages.iter().any(|m| m.id == id) {
            return Err(HostError("missing".into()));
        }
        Ok(SafeDocument {
            subject: id.into(),
            from: "sender@example.test".into(),
            text: "中".repeat(self.body_bytes / 3),
            links: vec![],
            attachments: vec![],
            truncated: false,
            warnings: vec![],
        })
    }
    fn create_draft(&self, _: AgentDraft) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn get_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_status(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
}
fn benchmark_draft(account: &str, from: &str, body_bytes: usize) -> DraftInput {
    DraftInput {
        account: account.into(),
        from: from.into(),
        to: vec!["recipient@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: "Benchmark editor".into(),
        body: "x".repeat(body_bytes.min(1024 * 1024 - 20)),
        attachments: vec![],
        in_reply_to: None,
        references: vec![],
    }
}
fn workspace_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    assert!(count == 1_000 || count == 10_000);
    let body_bytes = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    assert!(body_bytes == 32 * 1024 || body_bytes == 2 * 1024 * 1024);
    let messages = (0..count)
        .map(|i| MessageSummary {
            id: format!("mail-{i}"),
            subject: format!("Synthetic subject {i:05}"),
            from: "sender@example.test".into(),
        })
        .collect();
    let host: Arc<dyn Host> = Arc::new(BenchmarkHost {
        messages,
        body_bytes,
    });
    let dir = tempfile::tempdir().unwrap();
    let human = Arc::new(AppHost::open(&dir.path().join("private"), vec![], false, None).unwrap());
    human
        .create(benchmark_draft(
            "local-demo",
            "sender@example.test",
            body_bytes,
        ))
        .unwrap();
    let human: Arc<dyn HumanWorkspace> = human;
    run_tui_workspace(host, human).unwrap();
}

struct SyntheticVault;
impl CredentialStore for SyntheticVault {
    fn get(&self, _: &str, _: SecretKind) -> Result<Option<Secret>, AccountError> {
        Ok(Some(Secret::new("synthetic-secret".into())))
    }
    fn set(&self, _: &str, _: SecretKind, _: &Secret) -> Result<(), AccountError> {
        Ok(())
    }
    fn delete(&self, _: &str, _: SecretKind) -> Result<(), AccountError> {
        Ok(())
    }
}
fn workspace_tls_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_TLS_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    let raw_bytes: usize = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    let server = performance_server::Server::start(count, raw_bytes);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &server.root_pem).unwrap();
    let config: AccountConfig = serde_json::from_value(serde_json::json!({
        "id":"benchmark", "email":"person@example.test", "username":"person@example.test",
        "imap":{"host":"localhost","port":server.port},
        "smtp":{"host":"localhost","port":9465},
        "allowed_folders":["INBOX","Drafts"],
        "permissions":{"read":true,"draft":true,"send":false},
        "drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed",
        "authentication":{"type":"password"},"extra_ca_pem":ca,
    }))
    .unwrap();
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: std::env::var_os("LETTERKEEPER_BENCH_BINARY")
            .expect("explicit benchmark binary")
            .into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host = Arc::new(
        RemoteHost::open(
            config,
            &dir.path().join("state"),
            Arc::new(SyntheticVault),
            sandbox,
        )
        .unwrap(),
    );
    host.create(benchmark_draft(
        "benchmark",
        "person@example.test",
        raw_bytes,
    ))
    .unwrap();
    let browser: Arc<dyn Host> = host.clone();
    let human: Arc<dyn HumanWorkspace> = host;
    run_tui_workspace(browser, human).unwrap();
}

fn workspace_tls_mcp_benchmark_child() {
    let Ok(count) = std::env::var("LETTERKEEPER_BENCH_MCP_COUNT") else {
        return;
    };
    let count: usize = count.parse().unwrap();
    let raw_bytes: usize = std::env::var("LETTERKEEPER_BENCH_BODY_BYTES")
        .unwrap()
        .parse()
        .unwrap();
    let server = performance_server::Server::start(count, raw_bytes);
    let dir = tempfile::tempdir().unwrap();
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, &server.root_pem).unwrap();
    let config: AccountConfig = serde_json::from_value(serde_json::json!({
        "id":"benchmark", "email":"person@example.test", "username":"person@example.test",
        "imap":{"host":"localhost","port":server.port},
        "smtp":{"host":"localhost","port":9465}, "allowed_folders":["INBOX"],
        "permissions":{"read":true,"draft":false,"send":false},
        "drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"server_managed",
        "authentication":{"type":"password"},"extra_ca_pem":ca,
    }))
    .unwrap();
    let sandbox = letterkeeper::sandbox::Sandbox {
        executable: std::env::var_os("LETTERKEEPER_BENCH_BINARY")
            .expect("explicit benchmark binary")
            .into(),
        launcher: "/usr/bin/bwrap".into(),
    };
    let host: Arc<dyn Host> = Arc::new(
        RemoteHost::open(
            config,
            &dir.path().join("state"),
            Arc::new(SyntheticVault),
            sandbox,
        )
        .unwrap(),
    );
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(letterkeeper_adapters::run_mcp(host)).unwrap();
}
