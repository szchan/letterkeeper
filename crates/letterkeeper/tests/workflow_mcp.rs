// SPDX-License-Identifier: GPL-3.0-only
//! Shared local human/MCP state. All addresses and content are synthetic.
use letterkeeper::app::AppHost;
use letterkeeper_adapters::{
    McpAdapter,
    workspace::{HumanWorkspace, WorkflowError},
};
use letterkeeper_core::DraftInput;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn edited_input(mut input: DraftInput) -> DraftInput {
    input.cc = vec!["copy@example.test".into()];
    input.bcc = vec!["private@example.test".into()];
    input.subject = "中文主题".into();
    input.body = "第一行\n普通 q 与宽字符：你好".into();
    input
}

#[tokio::test]
async fn mcp_draft_is_human_editable_and_approval_never_sends() {
    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(AppHost::open(dir.path(), vec![], false, None).unwrap());
    let agent = McpAdapter::new(host.clone());
    let created = agent
        .call(
            "create_draft",
            json!({
                "to": ["recipient@example.test"],
                "subject": "Original",
                "body": "Synthetic content"
            }),
        )
        .await
        .unwrap();
    let id = created["id"].as_i64().unwrap();
    let listed = host.list_drafts(0, 50).unwrap();
    assert!(listed.iter().any(|draft| draft.id == id));
    let draft = host.load_draft(id).unwrap();
    let saved = host
        .save(id, draft.revision, edited_input(draft.input))
        .unwrap();
    assert_eq!(saved.input.cc, ["copy@example.test"]);
    assert_eq!(saved.input.bcc, ["private@example.test"]);
    let model_view = agent
        .call("get_draft", json!({"id": id.to_string()}))
        .await
        .unwrap();
    assert!(model_view.to_string().contains("中文主题"));
    assert_eq!(
        host.approve(id, &saved.digest, "wrong digest"),
        Err(WorkflowError::InvalidInput)
    );
    let approval = host.approve(id, &saved.digest, &saved.digest).unwrap();
    assert_eq!(approval.revision, saved.revision);
    assert!(
        host.list_submissions(0, 50).unwrap().is_empty(),
        "approval must not trigger SMTP"
    );
    assert!(
        agent
            .call("send_draft", json!({"id":id.to_string(), "confirm":true}))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn another_store_revision_invalidates_reviewed_digest() {
    let dir = tempfile::tempdir().unwrap();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let original = host
        .create(DraftInput {
            account: "local-demo".into(),
            from: "sender@example.test".into(),
            to: vec!["recipient@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Original".into(),
            body: "Safe".into(),
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        })
        .unwrap();
    let review = host.review(original.id).unwrap();
    let other = AppHost::open(dir.path(), vec![], false, None).unwrap();
    other
        .save(original.id, original.revision, edited_input(original.input))
        .unwrap();
    assert!(
        host.approve(original.id, &review.draft.digest, &review.draft.digest)
            .is_err()
    );
    assert!(host.send(original.id).is_err());
    assert!(host.list_submissions(0, 50).unwrap().is_empty());
}

#[test]
fn attachment_review_hashes_exact_bytes_and_large_body() {
    let dir = tempfile::tempdir().unwrap();
    let host = AppHost::open(dir.path(), vec![], false, None).unwrap();
    let original = host
        .create(DraftInput {
            account: "local-demo".into(),
            from: "sender@example.test".into(),
            to: vec!["recipient@example.test".into()],
            cc: vec![],
            bcc: vec![],
            subject: "long review".into(),
            body: "中文 q".repeat(5_000),
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        })
        .unwrap();
    let bytes = (0..=255).cycle().take(20_001).collect::<Vec<_>>();
    let file = dir.path().join("synthetic.bin");
    std::fs::write(&file, &bytes).unwrap();
    let added = host
        .add_attachment(
            original.id,
            original.revision,
            &file,
            "application/octet-stream",
        )
        .unwrap();
    let review = host.review(original.id).unwrap();
    assert!(review.draft.input.body.len() > 16 * 1024);
    assert_eq!(review.draft.digest, added.digest);
    assert_eq!(
        review.attachment_digests,
        vec![format!("{:x}", Sha256::digest(&bytes))]
    );
    let removed = host
        .remove_attachment(original.id, added.revision, 0)
        .unwrap();
    assert_ne!(removed.digest, review.draft.digest);
    assert!(
        host.approve(original.id, &review.draft.digest, &review.draft.digest)
            .is_err()
    );
}
