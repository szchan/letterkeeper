// SPDX-License-Identifier: GPL-3.0-only
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use letterkeeper_adapters::workspace::{ReviewSnapshot, WorkspaceCapabilities};
use letterkeeper_adapters::{WorkspaceState, review_text};
use letterkeeper_core::{Draft, DraftInput};
fn caps() -> WorkspaceCapabilities {
    WorkspaceCapabilities {
        account: "test".into(),
        from: "me@example.test".into(),
        read: true,
        draft: true,
        send: true,
    }
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
#[test]
fn editor_q_is_text_and_dirty_escape_requires_decision() {
    let mut s = WorkspaceState::new(caps());
    s.event(key(KeyCode::Char('c')));
    s.event(key(KeyCode::Char('q')));
    assert_eq!(s.editor.as_ref().unwrap().fields[0].lines(), &["q"]);
    s.event(key(KeyCode::Esc));
    assert!(s.dialog.is_some());
    assert!(!s.quit);
    s.event(key(KeyCode::Char('c')));
    assert!(s.editor.is_some());
}
#[test]
fn paste_is_atomic_and_dangerous_controls_rejected() {
    let mut s = WorkspaceState::new(caps());
    s.event(key(KeyCode::Char('c')));
    s.event(Event::Paste("中文q\nF9".into()));
    assert!(!s.quit);
    let before = s.editor.as_ref().unwrap().fields[0].lines().to_vec();
    s.event(Event::Paste("\x1b[31mSEND".into()));
    assert_eq!(s.editor.as_ref().unwrap().fields[0].lines(), before);
}
#[test]
fn full_review_preserves_tail_thread_and_escapes_reversibly() {
    let input = DraftInput {
        account: "a".into(),
        from: "m@example.test".into(),
        to: vec!["n@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: "\\\x1b".into(),
        body: format!("{}TAIL", "中".repeat(18000)),
        attachments: vec![],
        in_reply_to: Some("<thread@test>".into()),
        references: vec!["<ref@test>".into()],
    };
    let text = review_text(&ReviewSnapshot {
        draft: Draft {
            id: 1,
            revision: 2,
            digest: "digest".into(),
            input,
        },
        attachment_digests: vec![],
    });
    assert!(text.contains("TAIL"));
    assert!(text.contains("<thread@test>"));
    assert!(text.contains("<ref@test>"));
    assert!(text.contains("\\u{001b}"));
    assert!(!text.contains('\x1b'));
}
#[test]
fn body_budget_and_tab_focus_are_bounded() {
    let mut s = WorkspaceState::new(caps());
    s.event(key(KeyCode::Char('c')));
    for _ in 0..4 {
        s.event(key(KeyCode::Tab));
    }
    s.event(Event::Paste("x".repeat(1_048_576)));
    assert_eq!(
        s.editor.as_ref().unwrap().fields[4].lines()[0].len(),
        1_048_576
    );
    s.event(key(KeyCode::Char('x')));
    assert_eq!(
        s.editor.as_ref().unwrap().fields[4].lines()[0].len(),
        1_048_576
    );
}
#[test]
fn read_only_cannot_open_editor_or_approve() {
    let mut c = caps();
    c.draft = false;
    c.send = false;
    let mut s = WorkspaceState::new(c);
    s.event(key(KeyCode::Char('c')));
    assert!(s.editor.is_none());
    s.event(key(KeyCode::F(8)));
    assert!(s.dialog.is_none());
}

use letterkeeper_adapters::workspace::{
    HumanWorkspace, SubmissionInfo, WorkflowError, WorkflowResult,
};
use letterkeeper_adapters::{
    DraftEditor, WorkflowData, WorkflowJob, WorkflowResponse, WorkspaceView, WorkspaceWorker,
    render_workspace,
};
use letterkeeper_adapters::{Host, HostError, MessageSummary};
use letterkeeper_content::SafeDocument;
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
fn input() -> DraftInput {
    DraftInput {
        account: "test".into(),
        from: "me@example.test".into(),
        to: vec!["you@example.test".into()],
        cc: vec![],
        bcc: vec![],
        subject: "subject".into(),
        body: "body".into(),
        attachments: vec![],
        in_reply_to: Some("<thread@test>".into()),
        references: vec!["<ref@test>".into()],
    }
}
fn draft() -> Draft {
    Draft {
        id: 1,
        revision: 1,
        digest: "a".repeat(64),
        input: input(),
    }
}
fn response(job: WorkflowJob, result: WorkflowResult<WorkflowData>) -> WorkflowResponse {
    WorkflowResponse {
        generation: 0,
        edit_generation: 0,
        job,
        result,
    }
}
fn saved_state() -> WorkspaceState {
    let mut s = WorkspaceState::new(caps());
    s.editor = Some(DraftEditor::new(input(), Some(draft())));
    s.view = WorkspaceView::Editor;
    s
}
#[test]
fn save_ack_preserves_newer_editor_text_and_thread() {
    let mut s = saved_state();
    s.request(WorkflowJob::Save(1, 1, input()));
    let (g, e, j) = s.take_request(false).unwrap();
    s.event(key(KeyCode::Char('z')));
    let mut d = draft();
    d.revision = 2;
    s.complete(WorkflowResponse {
        generation: g,
        edit_generation: e,
        job: j,
        result: Ok(WorkflowData::Draft(Box::new(d))),
    });
    assert!(s.editor.as_ref().unwrap().dirty);
    assert!(s.editor.as_ref().unwrap().fields[0].lines()[0].contains('z'));
    assert_eq!(
        s.editor.as_ref().unwrap().saved.as_ref().unwrap().revision,
        2
    );
    assert_eq!(
        s.editor.as_ref().unwrap().input().in_reply_to,
        Some("<thread@test>".into())
    );
}
#[test]
fn conflict_retains_text_and_save_as_new_has_no_old_id() {
    let mut s = saved_state();
    s.event(key(KeyCode::Char('z')));
    s.complete(response(
        WorkflowJob::Save(1, 1, input()),
        Err(WorkflowError::Conflict),
    ));
    assert!(s.editor.as_ref().unwrap().fields[0].lines()[0].contains('z'));
    s.event(key(KeyCode::Char('n')));
    let (_, _, j) = s.take_request(false).unwrap();
    assert!(matches!(j, WorkflowJob::Create(_)));
}
#[test]
fn review_cancel_discards_read_result_but_send_result_survives() {
    let mut s = saved_state();
    s.event(key(KeyCode::Esc));
    assert_eq!(s.generation, 1);
    s.complete(response(
        WorkflowJob::Review(1),
        Ok(WorkflowData::Review(Box::new(ReviewSnapshot {
            draft: draft(),
            attachment_digests: vec![],
        }))),
    ));
    assert!(s.review.is_none());
    s.complete(response(
        WorkflowJob::Send(1),
        Ok(WorkflowData::Submission(SubmissionInfo {
            id: 7,
            draft_id: 1,
            state: letterkeeper_core::SubmissionState::Uncertain,
            state_recorded: true,
        })),
    ));
    assert_eq!(s.submissions[0].id, 7);
    assert!(s.status.contains("Uncertain"));
    assert!(s.take_request(true).is_none());
}
#[test]
fn approve_and_send_are_distinct_actions() {
    let mut s = saved_state();
    s.complete(response(
        WorkflowJob::Review(1),
        Ok(WorkflowData::Review(Box::new(ReviewSnapshot {
            draft: draft(),
            attachment_digests: vec![],
        }))),
    ));
    s.event(key(KeyCode::F(8)));
    s.event(Event::Paste("a".repeat(64)));
    s.event(key(KeyCode::Enter));
    let (_, _, job) = s.take_request(false).unwrap();
    assert!(matches!(job, WorkflowJob::Approve(1, _, _)));
    assert!(s.take_request(true).is_none());
    s.complete(response(
        job,
        Ok(WorkflowData::Approved(
            letterkeeper_adapters::workspace::ApprovalInfo {
                draft_id: 1,
                revision: 1,
                digest: "a".repeat(64),
                expires_at: 300,
            },
        )),
    ));
    assert!(s.take_request(true).is_none());
    s.event(key(KeyCode::F(9)));
    assert!(matches!(
        s.take_request(true).unwrap().2,
        WorkflowJob::Send(1)
    ));
}
#[test]
fn attachments_require_explicit_path_and_cancel_never_dispatches() {
    let mut s = saved_state();
    s.event(key(KeyCode::F(2)));
    s.event(Event::Paste("/tmp/synthetic.txt".into()));
    s.event(key(KeyCode::Esc));
    assert!(s.take_request(false).is_none());
    s.event(key(KeyCode::F(2)));
    s.event(Event::Paste("/tmp/synthetic.txt".into()));
    s.event(key(KeyCode::Enter));
    s.event(key(KeyCode::Enter));
    let (_, _, job) = s.take_request(false).unwrap();
    assert!(matches!(job, WorkflowJob::Add(1, 1, _, _)));
}
#[test]
fn four_sizes_chinese_combining_and_complete_review_tail() {
    for (width, height) in [(120, 36), (80, 24), (40, 12), (12, 5)] {
        let mut s = WorkspaceState::new(caps());
        s.event(key(KeyCode::Char('c')));
        for _ in 0..4 {
            s.event(key(KeyCode::Tab));
        }
        s.event(Event::Paste("中文e\u{0301}👩‍💻".into()));
        let mut t =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        t.draw(|f| render_workspace(f, &mut s)).unwrap();
        assert_eq!(s.editor.as_ref().unwrap().input().body, "中文e\u{0301}👩‍💻");
        let mut d = draft();
        d.input.body = format!("{}TAIL", "中".repeat(18000));
        s.editor = Some(DraftEditor::new(d.input.clone(), Some(d.clone())));
        let generation = s.generation;
        s.complete(WorkflowResponse {
            generation,
            edit_generation: 0,
            job: WorkflowJob::Review(1),
            result: Ok(WorkflowData::Review(Box::new(ReviewSnapshot {
                draft: d,
                attachment_digests: vec![],
            }))),
        });
        t.draw(|f| render_workspace(f, &mut s)).unwrap();
        s.event(key(KeyCode::End));
        t.draw(|f| render_workspace(f, &mut s)).unwrap();
        let buffer = t.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("TAIL"), "{width}x{height}");
    }
}
struct Mock {
    gate: (Mutex<bool>, Condvar),
    entered: AtomicBool,
    saved: AtomicBool,
}
impl HumanWorkspace for Mock {
    fn capabilities(&self) -> WorkspaceCapabilities {
        caps()
    }
    fn create(&self, input: DraftInput) -> WorkflowResult<Draft> {
        self.saved.store(true, Ordering::SeqCst);
        Ok(Draft {
            id: 1,
            revision: 1,
            digest: "a".repeat(64),
            input,
        })
    }
    fn send(&self, _: i64) -> WorkflowResult<SubmissionInfo> {
        self.entered.store(true, Ordering::SeqCst);
        let (mut lock, cv) = (self.gate.0.lock().unwrap(), &self.gate.1);
        while !*lock {
            lock = cv.wait(lock).unwrap();
        }
        Ok(SubmissionInfo {
            id: 1,
            draft_id: 1,
            state: letterkeeper_core::SubmissionState::Accepted,
            state_recorded: true,
        })
    }
}
impl Host for Mock {
    fn list_messages(&self, _: usize, _: usize) -> Result<Vec<MessageSummary>, HostError> {
        Ok(vec![])
    }
    fn read_message(&self, _: &str) -> Result<SafeDocument, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn create_draft(
        &self,
        _: letterkeeper_adapters::DraftInput,
    ) -> Result<serde_json::Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn get_draft(&self, _: &str) -> Result<serde_json::Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_draft(&self, _: &str) -> Result<serde_json::Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn send_status(&self, _: &str) -> Result<serde_json::Value, HostError> {
        Err(HostError("unsupported".into()))
    }
}
#[test]
fn network_block_does_not_block_local_save_and_worker_drop_does_not_cancel_send() {
    let m = Arc::new(Mock {
        gate: (Mutex::new(false), Condvar::new()),
        entered: AtomicBool::new(false),
        saved: AtomicBool::new(false),
    });
    let mut worker = WorkspaceWorker::new(m.clone(), m.clone());
    let mut s = saved_state();
    assert!(s.request(WorkflowJob::Send(1)));
    worker.poll(&mut s);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !m.entered.load(Ordering::SeqCst) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(s.request(WorkflowJob::Create(input())));
    worker.poll(&mut s);
    while !m.saved.load(Ordering::SeqCst) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(s.network_busy);
    assert!(!s.request(WorkflowJob::Send(1)));
    drop(worker);
    *m.gate.0.lock().unwrap() = true;
    m.gate.1.notify_all();
}
