// SPDX-License-Identifier: GPL-3.0-only
use crossterm::event::KeyCode;
use letterkeeper_adapters::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
struct Mock {
    calls: Mutex<Vec<String>>,
}
impl Mock {
    fn record(&self, s: String) {
        self.calls.lock().unwrap().push(s);
    }
}
impl Host for Mock {
    fn list_messages(&self, _: usize, _: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(vec![])
    }
    fn read_message(&self, _: &str) -> Result<letterkeeper_content::SafeDocument, HostError> {
        Err(HostError("unavailable".into()))
    }
    fn create_draft(&self, _: DraftInput) -> Result<Value, HostError> {
        Ok(json!({}))
    }
    fn get_draft(&self, _: &str) -> Result<Value, HostError> {
        Ok(json!({}))
    }
    fn send_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unauthorized".into()))
    }
    fn send_status(&self, _: &str) -> Result<Value, HostError> {
        Ok(json!({}))
    }
    fn list_accounts(&self) -> Result<Value, HostError> {
        Ok(json!({"accounts":[{"id":"scope_demo","name":"<img https://evil.test>"}]}))
    }
    fn list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        Ok(vec![FolderSummary {
            id: "folder_1".into(),
            name: "![x](https://evil.test)".into(),
        }])
    }
    fn list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.record(format!("list:{folder:?}:{offset}:{limit}"));
        Ok(vec![MessageSummary {
            id: "mail_1".into(),
            subject: "<img https://evil.test>".into(),
            from: "a@test.test".into(),
        }])
    }
    fn search_messages(
        &self,
        folder: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.record(format!("search:{folder:?}:{query}:{offset}:{limit}"));
        Ok(vec![])
    }
    fn reply_draft(
        &self,
        id: &str,
        mode: &str,
        body: &str,
        to: &[String],
    ) -> Result<Value, HostError> {
        self.record(format!("reply:{id}:{mode}:{body}:{to:?}"));
        Ok(json!({"id":"draft_1","body":"![x](https://evil.test)"}))
    }
    fn sync_draft(&self, id: &str) -> Result<Value, HostError> {
        self.record(format!("sync_draft:{id}"));
        Ok(json!({"id":id}))
    }
    fn sync_sent(&self, id: &str) -> Result<Value, HostError> {
        self.record(format!("sync_sent:{id}"));
        Ok(json!({"id":id}))
    }
    fn sync_status(&self, id: &str) -> Result<Value, HostError> {
        self.record(format!("sync_status:{id}"));
        Ok(json!({"id":id}))
    }
    fn sent_evidence(&self, id: &str) -> Result<Value, HostError> {
        self.record(format!("sent_evidence:{id}"));
        Ok(json!({"id":id}))
    }
}
fn mock() -> Arc<Mock> {
    Arc::new(Mock {
        calls: Mutex::new(vec![]),
    })
}
#[tokio::test]
async fn new_routes_are_scoped_and_opaque_ids_survive_inert_output() {
    let h = mock();
    let a = McpAdapter::new(h.clone());
    assert_eq!(
        a.call("list_accounts", json!({})).await.unwrap()["accounts"][0]["id"],
        "scope_demo"
    );
    assert_eq!(
        a.call("list_folders", json!({})).await.unwrap()["folders"][0]["id"],
        "folder_1"
    );
    let list = a
        .call("list_messages", json!({"folder":"folder_1","limit":2}))
        .await
        .unwrap();
    assert_eq!(list["messages"][0]["id"], "mail_1");
    assert!(!list.to_string().contains("https://"));
    a.call(
        "search_messages",
        json!({"folder":"folder_1","query":"中文","offset":2,"limit":3}),
    )
    .await
    .unwrap();
    let reply = a
        .call(
            "reply_draft",
            json!({"id":"mail_1","mode":"reply_all","body":"答复"}),
        )
        .await
        .unwrap();
    assert_eq!(reply["id"], "draft_1");
    assert!(!reply.to_string().contains("!["));
    for name in ["sync_draft", "sync_sent", "sync_status", "sent_evidence"] {
        assert_eq!(
            a.call(name, json!({"id":"draft_1"})).await.unwrap()["id"],
            "draft_1"
        );
    }
    let calls = h.calls.lock().unwrap();
    assert!(
        calls
            .iter()
            .any(|s| s == "search:Some(\"folder_1\"):中文:2:3")
    );
}
#[tokio::test]
async fn invalid_new_arguments_never_reach_host() {
    let h = mock();
    let a = McpAdapter::new(h.clone());
    for (name, args) in [
        ("list_accounts", json!({"account":"other"})),
        ("list_folders", json!({"path":"/tmp"})),
        ("list_messages", json!({"folder":"../secret"})),
        ("search_messages", json!({"query":"x".repeat(1025)})),
        ("search_messages", json!({"query":"x\r\nUID SEARCH ALL"})),
        ("search_messages", json!({"query":"x","limit":51})),
        (
            "reply_draft",
            json!({"id":"m","mode":"reply","body":"x","approve":true}),
        ),
        ("reply_draft", json!({"id":"m","mode":"send","body":"x"})),
        (
            "reply_draft",
            json!({"id":"m","mode":"reply","body":"x".repeat(32769)}),
        ),
        ("sync_sent", json!({"id":"m","account":"other"})),
        ("sent_evidence", json!({"id":"/etc/passwd"})),
    ] {
        assert!(a.call(name, args).await.is_err(), "{name}");
    }
    assert!(h.calls.lock().unwrap().is_empty());
}
#[test]
fn browser_cancellation_discards_stale_results_and_single_flight() {
    let mut state = BrowserState::default();
    let (generation, _) = state.take_request().expect("initial folders request");
    assert!(state.take_request().is_none());
    state.key(KeyCode::Esc);
    assert!(state.take_request().is_none());
    state.complete(BrowserResponse {
        generation,
        result: Ok(BrowserData::Folders(vec![FolderSummary {
            id: "stale".into(),
            name: "old".into(),
        }])),
    });
    assert!(state.folders.is_empty());
    assert!(!state.busy);
    state.key(KeyCode::Char('r'));
    assert!(state.take_request().is_some());
    state.key(KeyCode::Char('q'));
    assert!(state.ui.quit);
}

#[tokio::test]
async fn official_sdk_lists_schemas_and_calls_new_tools() {
    use rmcp::ServiceExt;
    let (server_io, client_io) = tokio::io::duplex(65536);
    let server =
        tokio::spawn(async move { McpAdapter::new(mock()).serve(server_io).await.unwrap() });
    let client = ().serve(client_io).await.unwrap();
    let server = server.await.unwrap();
    let tools = client.list_tools(Default::default()).await.unwrap();
    assert_eq!(tools.tools.len(), 14);
    for tool in &tools.tools {
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert!(!tool.name.contains("approve"));
    }
    let reply = tools
        .tools
        .iter()
        .find(|t| t.name == "reply_draft")
        .unwrap();
    assert_eq!(
        reply.input_schema["properties"]["mode"]["enum"],
        json!(["reply", "reply_all", "forward"])
    );
    for (name, args) in [
        ("list_folders", json!({})),
        ("search_messages", json!({"query":"中文"})),
        (
            "reply_draft",
            json!({"id":"mail_1","mode":"reply","body":"答复"}),
        ),
        ("sync_status", json!({"id":"draft_1"})),
    ] {
        let response = client
            .call_tool(
                rmcp::model::CallToolRequestParams::new(name)
                    .with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        let text = serde_json::to_string(&response).unwrap();
        assert!(text.contains("untrusted"));
        assert!(!text.contains("https://"));
    }
    client.cancel().await.unwrap();
    server.cancel().await.unwrap();
}
#[test]
fn browser_pages_search_read_and_tiny_layout() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = BrowserState::default();
    let (generation, _) = state.take_request().unwrap();
    state.complete(BrowserResponse {
        generation,
        result: Ok(BrowserData::Folders(vec![
            FolderSummary {
                id: "folder_1".into(),
                name: "中文文件夹".into(),
            },
            FolderSummary {
                id: "folder_2".into(),
                name: "另一个".into(),
            },
        ])),
    });
    let (generation, job) = state.take_request().unwrap();
    assert!(matches!(job, BrowserJob::List { offset: 0, .. }));
    state.complete(BrowserResponse {
        generation,
        result: Ok(BrowserData::List(
            (0..50)
                .map(|i| MessageSummary {
                    id: format!("m_{i}"),
                    subject: "中文📨".into(),
                    from: "a@example.test".into(),
                })
                .collect(),
        )),
    });
    state.key(KeyCode::Char('n'));
    let (generation, job) = state.take_request().unwrap();
    assert!(matches!(job, BrowserJob::List { offset: 50, .. }));
    state.key(KeyCode::Char('p'));
    assert!(state.take_request().is_none());
    state.complete(BrowserResponse {
        generation,
        result: Ok(BrowserData::List(vec![])),
    });
    assert_eq!(state.messages.len(), 50); // stale page must not replace current list
    let (generation, job) = state.take_request().unwrap();
    assert!(matches!(job, BrowserJob::List { offset: 0, .. }));
    state.complete(BrowserResponse {
        generation,
        result: Ok(BrowserData::List(vec![MessageSummary {
            id: "m_1".into(),
            subject: "中文".into(),
            from: "a".into(),
        }])),
    });
    state.key(KeyCode::Enter);
    let (generation, job) = state.take_request().unwrap();
    assert!(matches!(job, BrowserJob::Read(_)));
    state.complete(BrowserResponse {
        generation,
        result: Err(HostError("secret token".into())),
    });
    assert!(!state.status.contains("secret"));
    state.key(KeyCode::Esc);
    state.key(KeyCode::Char('f'));
    assert_eq!(state.folder.as_deref(), Some("folder_2"));
    state.key(KeyCode::Char('/'));
    state.key(KeyCode::Char('中'));
    state.key(KeyCode::Enter);
    let (_, job) = state.take_request().unwrap();
    assert!(matches!(job,BrowserJob::List {query,..} if query=="中"));
    for (w, h) in [(100, 25), (24, 10), (1, 1), (0, 0)] {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| render_dynamic(f, &state)).unwrap();
    }
}
struct Slow {
    started: std::sync::mpsc::SyncSender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    calls: std::sync::atomic::AtomicUsize,
}
impl Host for Slow {
    fn list_messages(&self, _: usize, _: usize) -> Result<Vec<MessageSummary>, HostError> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.started.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(vec![])
    }
    fn read_message(&self, _: &str) -> Result<letterkeeper_content::SafeDocument, HostError> {
        Err(HostError("unavailable".into()))
    }
    fn create_draft(&self, _: DraftInput) -> Result<Value, HostError> {
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
#[test]
fn blocked_host_does_not_block_escape_quit_or_spawn_more_work() {
    use std::{
        sync::atomic::Ordering,
        sync::mpsc,
        time::{Duration, Instant},
    };
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let host = Arc::new(Slow {
        started: started_tx,
        release: Mutex::new(release_rx),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let mut worker = BrowserWorker::new(host.clone());
    let mut state = BrowserState::default();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        worker.poll(&mut state);
        if started_rx.try_recv().is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let begin = Instant::now();
    state.key(KeyCode::Esc);
    for _ in 0..100 {
        state.key(KeyCode::Char('r'));
        worker.poll(&mut state);
    }
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    state.key(KeyCode::Char('q'));
    assert!(state.ui.quit);
    drop(worker);
    assert!(begin.elapsed() < Duration::from_millis(500));
    release_tx.send(()).unwrap();
}

#[tokio::test]
async fn forward_requires_explicit_recipients_and_reply_cannot_override_reply_to() {
    let h = mock();
    let a = McpAdapter::new(h.clone());
    for args in [
        json!({"id":"m","mode":"forward","body":"x"}),
        json!({"id":"m","mode":"reply","body":"x","to":["evil@example.test"]}),
        json!({"id":"m","mode":"reply_all","body":"x","to":["evil@example.test"]}),
        json!({"id":"m","mode":"forward","body":"x","to":["x\r\nBcc: injected@example.test"]}),
        json!({"id":"m","mode":"forward","body":"x","to":vec!["a@example.test";21]}),
    ] {
        assert!(a.call("reply_draft", args).await.is_err());
    }
    assert!(h.calls.lock().unwrap().is_empty());
    a.call(
        "reply_draft",
        json!({"id":"m","mode":"forward","body":"x","to":["recipient@example.test"]}),
    )
    .await
    .unwrap();
    assert!(
        h.calls
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.contains("recipient@example.test"))
    );
}

struct ConcurrentHost {
    entered: tokio::sync::mpsc::UnboundedSender<String>,
    released: Mutex<bool>,
    ready: std::sync::Condvar,
}
impl ConcurrentHost {
    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.ready.notify_all();
    }
}
struct ReleaseOnDrop(Arc<ConcurrentHost>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}
impl Host for ConcurrentHost {
    fn list_messages(&self, _: usize, _: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(vec![])
    }
    fn read_message(&self, _: &str) -> Result<letterkeeper_content::SafeDocument, HostError> {
        Err(HostError("synthetic error".into()))
    }
    fn create_draft(&self, _: DraftInput) -> Result<Value, HostError> {
        Ok(json!({}))
    }
    fn get_draft(&self, id: &str) -> Result<Value, HostError> {
        if id == "fail" {
            return Err(HostError("synthetic error".into()));
        }
        if id == "panic" {
            panic!("synthetic host failure");
        }
        self.entered.send(id.to_owned()).unwrap();
        let mut released = self.released.lock().unwrap();
        while !*released {
            released = self.ready.wait(released).unwrap();
        }
        Ok(json!({"id":id}))
    }
    fn send_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unauthorized".into()))
    }
    fn send_status(&self, _: &str) -> Result<Value, HostError> {
        Ok(json!({}))
    }
}
#[tokio::test]
async fn mcp_shared_quota_rejects_ninth_job_and_cancellation_retains_permit() {
    use std::time::Duration;
    let (entered, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let host = Arc::new(ConcurrentHost {
        entered,
        released: Mutex::new(false),
        ready: std::sync::Condvar::new(),
    });
    let _cleanup = ReleaseOnDrop(host.clone());
    let adapter = McpAdapter::new(host.clone());
    let mut jobs = Vec::new();
    for index in 0..8 {
        let cloned = adapter.clone();
        jobs.push(tokio::spawn(async move {
            cloned
                .call("get_draft", json!({"id":format!("blocked_{index}")}))
                .await
        }));
    }
    let mut started = std::collections::HashSet::new();
    for _ in 0..8 {
        started.insert(
            tokio::time::timeout(Duration::from_secs(2), receive.recv())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(
        started.len(),
        8,
        "positive control: all eight host calls entered"
    );
    let ninth = tokio::time::timeout(
        Duration::from_millis(250),
        adapter.clone().call("get_draft", json!({"id":"ninth"})),
    )
    .await
    .expect("quota must reject immediately without queuing");
    assert_eq!(ninth.unwrap_err().0, "host busy");
    let cancelled = jobs.remove(0);
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    let ninth = tokio::time::timeout(
        Duration::from_millis(250),
        adapter.call("get_draft", json!({"id":"after_cancel"})),
    )
    .await
    .expect("cancelled async call must keep its blocked host slot");
    assert_eq!(ninth.unwrap_err().0, "host busy");
    assert!(
        receive.try_recv().is_err(),
        "rejected calls never entered Host"
    );
    host.release();
    for job in jobs {
        assert!(job.await.unwrap().is_ok());
    }
    assert_eq!(
        adapter
            .call("get_draft", json!({"id":"reuse"}))
            .await
            .unwrap()["id"],
        "reuse"
    );
}
#[tokio::test]
async fn mcp_quota_releases_permits_after_host_error_and_panic() {
    let (entered, _receive) = tokio::sync::mpsc::unbounded_channel();
    let host = Arc::new(ConcurrentHost {
        entered,
        released: Mutex::new(true),
        ready: std::sync::Condvar::new(),
    });
    let adapter = McpAdapter::new(host);
    for _ in 0..12 {
        assert_eq!(
            adapter
                .call("get_draft", json!({"id":"fail"}))
                .await
                .unwrap_err()
                .0,
            "synthetic error"
        );
        assert_eq!(
            adapter
                .clone()
                .call("get_draft", json!({"id":"panic"}))
                .await
                .unwrap_err()
                .0,
            "host unavailable"
        );
    }
    assert_eq!(
        adapter
            .call("get_draft", json!({"id":"reuse"}))
            .await
            .unwrap()["id"],
        "reuse"
    );
}
