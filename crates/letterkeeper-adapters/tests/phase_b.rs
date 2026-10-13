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
    fn reply_draft(&self, id: &str, mode: &str, body: &str) -> Result<Value, HostError> {
        self.record(format!("reply:{id}:{mode}:{body}"));
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
