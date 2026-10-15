// SPDX-License-Identifier: GPL-3.0-only
//! Human-only workbench; no instance of this capability enters MCP.
use crate::{workspace::*, *};
use crossterm::event::{KeyEventKind, KeyModifiers};
use letterkeeper_core::{Draft, DraftInput as CoreInput, SendDiagnostic};
use std::{path::PathBuf, sync::mpsc};
use tui_textarea::TextArea;

pub const MAX_EDITOR_BODY: usize = 1_048_576;
/// Lossless terminal-safe spelling: backslashes and dangerous Unicode are escaped.
/// Newlines are retained for scrolling; all other controls have a reversible spelling.
pub fn review_escape(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        if c == '\\' {
            result.push_str("\\\\");
        } else if dangerous(c) || (c.is_control() && c != '\n') {
            result.push_str(&format!("\\u{{{:04x}}}", c as u32));
        } else {
            result.push(c);
        }
    }
    result
}
fn dangerous(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}
pub fn review_text(review: &ReviewSnapshot) -> String {
    let draft = &review.draft;
    let i = &draft.input;
    let mut text = format!(
        "完整审阅 · 草稿 {} 版本 {}\n摘要: {}\nAccount: {}\nFrom: {}\nTo: {}\nCc: {}\nBcc: {}\nSubject: {}\nIn-Reply-To: {}\nReferences: {}\nAttachments: {}\n",
        draft.id,
        draft.revision,
        draft.digest,
        review_escape(&i.account),
        review_escape(&i.from),
        i.to.iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        i.cc.iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        i.bcc
            .iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(", "),
        review_escape(&i.subject),
        i.in_reply_to
            .as_deref()
            .map(review_escape)
            .unwrap_or_default(),
        i.references
            .iter()
            .map(|s| review_escape(s))
            .collect::<Vec<_>>()
            .join(" "),
        i.attachments.len()
    );
    for (index, attachment) in i.attachments.iter().enumerate() {
        text.push_str(&format!(
            "{}: {} | {} | {} bytes | SHA-256 {}\n",
            index,
            review_escape(&attachment.name),
            review_escape(&attachment.content_type),
            attachment.bytes.len(),
            review
                .attachment_digests
                .get(index)
                .map(String::as_str)
                .unwrap_or("MISSING HASH — approval unavailable")
        ));
    }
    text.push_str("\nBody (exact; \\\\ = backslash, \\u{xxxx} = escaped control):\n");
    text.push_str(&review_escape(&i.body));
    text
}

pub struct DraftEditor {
    pub fields: [TextArea<'static>; 5],
    pub focus: usize,
    pub saved: Option<Draft>,
    pub fixed: CoreInput,
    pub dirty: bool,
    pub generation: u64,
}
impl DraftEditor {
    pub fn new(input: CoreInput, saved: Option<Draft>) -> Self {
        let values = [
            input.to.join(", "),
            input.cc.join(", "),
            input.bcc.join(", "),
            input.subject.clone(),
            input.body.clone(),
        ];
        let fields = values.map(|v| {
            let mut field = TextArea::from(v.split('\n').map(str::to_owned).collect::<Vec<_>>());
            field.set_max_histories(20);
            field.set_style(Style::default().fg(Color::White).bg(Color::Black));
            field
        });
        Self {
            fields,
            focus: 0,
            saved,
            fixed: input,
            dirty: false,
            generation: 0,
        }
    }
    pub fn input(&self) -> CoreInput {
        let mut input = self.fixed.clone();
        let addresses = |index: usize| {
            self.fields[index]
                .lines()
                .join(" ")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        input.to = addresses(0);
        input.cc = addresses(1);
        input.bcc = addresses(2);
        input.subject = self.fields[3].lines().join(" ");
        input.body = self.fields[4].lines().join("\n");
        input
    }
    fn edit(&mut self, event: Event) -> Result<(), &'static str> {
        let before = self.fields[self.focus].clone();
        let changed = match event {
            Event::Paste(text) => {
                if text
                    .chars()
                    .any(|c| dangerous(c) || (c.is_control() && c != '\n' && c != '\t'))
                {
                    return Err("拒绝危险粘贴控制字符");
                }
                if self.focus != 4 && text.contains(['\n', '\t']) {
                    return Err("地址和主题不接受多行粘贴");
                }
                if text.len() > MAX_EDITOR_BODY {
                    return Err("正文超过 1 MiB");
                }
                self.fields[self.focus].insert_str(text)
            }
            Event::Key(key) => {
                if let KeyCode::Char(c) = key.code
                    && (dangerous(c) || c.is_control())
                {
                    return Err("拒绝危险字符");
                }
                if self.focus != 4 && key.code == KeyCode::Enter {
                    return Ok(());
                }
                self.fields[self.focus].input(key)
            }
            _ => false,
        };
        let len = self.fields[self.focus]
            .lines()
            .iter()
            .map(String::len)
            .sum::<usize>()
            + self.fields[self.focus].lines().len().saturating_sub(1);
        let max = if self.focus == 4 {
            MAX_EDITOR_BODY
        } else if self.focus == 3 {
            256
        } else {
            20 * 256
        };
        if len > max {
            self.fields[self.focus] = before;
            return Err("编辑字段超过限制");
        }
        if changed {
            self.dirty = true;
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum WorkflowJob {
    Drafts(usize),
    Load(i64),
    Create(CoreInput),
    Save(i64, i64, CoreInput),
    Review(i64),
    Approve(i64, String, String),
    Send(i64),
    Reply(String, ReplyMode, Vec<String>),
    Add(i64, i64, PathBuf, String),
    Remove(i64, i64, usize),
    Export(String, usize, PathBuf),
    Submissions(usize),
    Status(i64),
    Evidence(i64),
    Recover(i64),
    SyncDraft(i64),
    SyncSent(i64),
    SyncStatus(i64),
}
impl WorkflowJob {
    fn network(&self) -> bool {
        matches!(
            self,
            Self::Send(..)
                | Self::Reply(..)
                | Self::Export(..)
                | Self::Evidence(..)
                | Self::SyncDraft(..)
                | Self::SyncSent(..)
        )
    }
    fn readonly(&self) -> bool {
        matches!(
            self,
            Self::Drafts(..)
                | Self::Load(..)
                | Self::Review(..)
                | Self::Submissions(..)
                | Self::Status(..)
                | Self::Evidence(..)
                | Self::SyncStatus(..)
        )
    }
}
pub enum WorkflowData {
    Drafts(Vec<DraftSummary>),
    Draft(Box<Draft>),
    Review(Box<ReviewSnapshot>),
    Approved(ApprovalInfo),
    Submissions(Vec<SubmissionInfo>),
    Submission(SubmissionInfo),
    SubmissionDiagnostic(SubmissionInfo, Option<SendDiagnostic>),
    Evidence(SentEvidence),
    Sync(SyncInfo),
    Exported(String),
}
pub struct WorkflowResponse {
    pub generation: u64,
    pub edit_generation: u64,
    pub job: WorkflowJob,
    pub result: WorkflowResult<WorkflowData>,
}
#[derive(Clone, Debug)]
pub enum WorkspaceDialog {
    Dirty { quit: bool },
    Conflict,
    Approve(String),
    AddPath(String),
    AddType(PathBuf, String),
    Remove(String),
    Forward(String, String),
    ExportPart(String, String),
    ExportDirectory(String, usize, String),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceView {
    Mail,
    Drafts,
    Submissions,
    Editor,
    Review,
}
pub struct WorkspaceState {
    pub browser: BrowserState,
    pub capabilities: WorkspaceCapabilities,
    pub editor: Option<DraftEditor>,
    pub review: Option<ReviewSnapshot>,
    pub review_lines: Vec<String>,
    pub review_wrapped: Vec<String>,
    pub review_width: usize,
    pub dialog: Option<WorkspaceDialog>,
    pub view: WorkspaceView,
    pub drafts: Vec<DraftSummary>,
    pub submissions: Vec<SubmissionInfo>,
    pub selected: usize,
    pub offset: usize,
    pub scroll: usize,
    pub horizontal: usize,
    pub status: String,
    pub quit: bool,
    pub local_busy: bool,
    pub network_busy: bool,
    pub generation: u64,
    pub sync_id: Option<i64>,
    pending_local: Option<WorkflowJob>,
    pending_network: Option<WorkflowJob>,
    after_save: Option<bool>,
}
impl WorkspaceState {
    fn show_submission(&mut self, info: SubmissionInfo, diagnostic: Option<SendDiagnostic>) {
        if let Some(old) = self.submissions.iter_mut().find(|s| s.id == info.id) {
            *old = info.clone();
        } else {
            self.submissions.insert(0, info.clone());
            self.submissions.truncate(50);
        }
        let detail = if info.state_recorded {
            diagnostic
                .map(|d| {
                    // Serde names are fixed enum codes, never server strings.
                    let stage = serde_json::to_value(d.stage).expect("enum serializes");
                    let kind = serde_json::to_value(d.kind).expect("enum serializes");
                    format!(
                        "{} / {}{}",
                        stage.as_str().unwrap_or("unknown"),
                        kind.as_str().unwrap_or("unknown"),
                        d.smtp_code
                            .map(|code| format!(" / {code}"))
                            .unwrap_or_default()
                    )
                })
                .unwrap_or_else(|| "unknown（无诊断记录）".into())
        } else {
            "unknown（状态未落盘）".into()
        };
        self.status = format!(
            "提交 {} {:?} · durable={} · 诊断: {}\n不自动重试 · s 查看记录",
            info.id, info.state, info.state_recorded, detail
        );
    }
    pub fn new(capabilities: WorkspaceCapabilities) -> Self {
        Self {
            browser: BrowserState::default(),
            capabilities,
            editor: None,
            review: None,
            review_lines: vec![],
            review_wrapped: vec![],
            review_width: 0,
            dialog: None,
            view: WorkspaceView::Mail,
            drafts: vec![],
            submissions: vec![],
            selected: 0,
            offset: 0,
            scroll: 0,
            horizontal: 0,
            status: "就绪".into(),
            quit: false,
            local_busy: false,
            network_busy: false,
            generation: 0,
            sync_id: None,
            pending_local: None,
            pending_network: None,
            after_save: None,
        }
    }
    fn empty(&self) -> CoreInput {
        CoreInput {
            account: self.capabilities.account.clone(),
            from: self.capabilities.from.clone(),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: String::new(),
            body: String::new(),
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        }
    }
    pub fn request(&mut self, job: WorkflowJob) -> bool {
        let network = job.network();
        if (network && (self.network_busy || self.pending_network.is_some()))
            || (!network && (self.local_busy || self.pending_local.is_some()))
        {
            self.status = "工作线程忙；稍后显式操作".into();
            return false;
        }
        if network {
            self.pending_network = Some(job);
        } else {
            self.pending_local = Some(job);
        }
        self.status = "操作加载中 · 退出不取消已接收的保存/发送".into();
        true
    }
    pub fn take_request(&mut self, network: bool) -> Option<(u64, u64, WorkflowJob)> {
        if if network {
            self.network_busy
        } else {
            self.local_busy
        } {
            return None;
        }
        let job = if network {
            self.pending_network.take()
        } else {
            self.pending_local.take()
        }?;
        if network {
            self.network_busy = true;
        } else {
            self.local_busy = true;
        }
        Some((
            self.generation,
            self.editor.as_ref().map(|e| e.generation).unwrap_or(0),
            job,
        ))
    }
    pub fn complete(&mut self, response: WorkflowResponse) {
        if response.job.network() {
            self.network_busy = false;
        } else {
            self.local_busy = false;
        }
        if response.job.readonly() && response.generation != self.generation {
            return;
        }
        match response.result {
            Err(WorkflowError::Conflict) => {
                if response.generation != self.generation {
                    self.status = "旧编辑会话保存失败：版本冲突；原编辑未覆盖".into();
                    return;
                }
                self.status = "版本冲突；保留编辑内容".into();
                self.dialog = Some(WorkspaceDialog::Conflict);
                self.after_save = None;
            }
            Err(e) => {
                self.status = format!("操作失败: {e}");
                self.after_save = None;
            }
            Ok(WorkflowData::Drafts(drafts)) => {
                self.drafts = drafts.into_iter().take(50).collect();
                self.selected = 0;
                self.status = "草稿已加载".into();
            }
            Ok(WorkflowData::Draft(draft)) => {
                if !response.job.readonly() && response.generation != self.generation {
                    self.status = format!(
                        "后台已保存草稿 {} 版本 {}；d 查询",
                        draft.id, draft.revision
                    );
                    return;
                }
                let same_editor = self
                    .editor
                    .as_ref()
                    .is_some_and(|e| e.generation == response.edit_generation);
                if matches!(response.job, WorkflowJob::Load(..))
                    && self.editor.is_some()
                    && !same_editor
                {
                    self.status = "重新加载返回期间有新输入；保留文本，请再次选择冲突处理".into();
                    self.dialog = Some(WorkspaceDialog::Conflict);
                    return;
                }
                if matches!(
                    response.job,
                    WorkflowJob::Save(..) | WorkflowJob::Create(..)
                ) && self.editor.is_some()
                {
                    let e = self.editor.as_mut().unwrap();
                    e.saved = Some((*draft).clone());
                    e.fixed.attachments = draft.input.attachments.clone();
                    if same_editor {
                        e.dirty = false;
                        e.fixed = draft.input.clone();
                    }
                    self.review = None;
                    self.status = format!(
                        "已保存草稿 {} 版本 {}{}",
                        draft.id,
                        draft.revision,
                        if same_editor {
                            ""
                        } else {
                            "；有后续编辑尚未保存"
                        }
                    );
                    if same_editor && let Some(quit) = self.after_save.take() {
                        self.editor = None;
                        self.view = WorkspaceView::Mail;
                        self.quit = quit;
                    } else {
                        self.after_save = None;
                    }
                } else {
                    self.editor = Some(DraftEditor::new(draft.input.clone(), Some(*draft)));
                    self.view = WorkspaceView::Editor;
                    self.review = None;
                    self.status = "草稿已加载 · Ctrl+S 保存".into();
                }
            }
            Ok(WorkflowData::Review(review)) => {
                if self.editor.as_ref().is_some_and(|e| e.dirty) {
                    self.status = "审阅返回期间内容已修改；请保存后重新审阅".into();
                    return;
                }
                self.review_lines = review_text(&review)
                    .split('\n')
                    .map(str::to_owned)
                    .collect();
                self.review = Some(*review);
                self.review_width = 0;
                self.scroll = 0;
                self.horizontal = 0;
                self.view = WorkspaceView::Review;
                self.status = "完整审阅 · ↑↓/PgUp/PgDn 滚动 · ←→ 横移 · F8 输入完整摘要".into();
            }
            Ok(WorkflowData::Approved(info)) => {
                self.status = format!("批准成功 · 300 秒 · 到期 {} · F9 单独发送", info.expires_at);
            }
            Ok(WorkflowData::Submissions(items)) => {
                self.submissions = items.into_iter().take(50).collect();
                self.selected = 0;
                self.status = "提交状态 · Accepted 仅 SMTP 接受；Uncertain 禁止自动重试".into();
            }
            Ok(WorkflowData::Submission(info)) => self.show_submission(info, None),
            Ok(WorkflowData::SubmissionDiagnostic(info, diagnostic)) => {
                self.show_submission(info, diagnostic)
            }
            Ok(WorkflowData::Evidence(info)) => {
                self.status = format!(
                    "Sent 观察证据: 提交 {} 匹配 {} 封；不证明投递",
                    info.submission_id, info.matching_messages
                );
            }
            Ok(WorkflowData::Sync(info)) => {
                self.sync_id = Some(info.id);
                self.status = format!("同步 {} {:?} · v 查询 · 不自动重试", info.id, info.state);
            }
            Ok(WorkflowData::Exported(path)) => {
                self.status = format!("附件已保存: {}", review_escape(&path));
            }
        }
    }
    fn save(&mut self, as_new: bool) -> bool {
        if !self.capabilities.draft {
            self.status = "起草权限被拒绝".into();
            return false;
        }
        let Some(e) = self.editor.as_ref() else {
            return false;
        };
        let input = e.input();
        let job = if !as_new && let Some(d) = e.saved.as_ref() {
            WorkflowJob::Save(d.id, d.revision, input)
        } else {
            WorkflowJob::Create(input)
        };
        self.request(job)
    }
    fn saved_id(&self) -> Option<i64> {
        self.editor
            .as_ref()
            .filter(|e| !e.dirty)
            .and_then(|e| e.saved.as_ref())
            .map(|d| d.id)
    }
    fn leave(&mut self, quit: bool) {
        if self.editor.as_ref().is_some_and(|e| e.dirty) {
            self.dialog = Some(WorkspaceDialog::Dirty { quit });
        } else {
            self.generation = self.generation.wrapping_add(1);
            self.editor = None;
            self.review = None;
            self.view = WorkspaceView::Mail;
            self.quit = quit;
        }
    }
    pub fn event(&mut self, event: Event) {
        if let Event::Key(key) = &event
            && key.kind != KeyEventKind::Press
        {
            return;
        }
        if self.dialog.is_some() {
            self.dialog_event(event);
            return;
        }
        if let Event::Key(key) = &event {
            if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.leave(true);
                return;
            }
            if key.code == KeyCode::F(6) {
                if let Some(id) = self.saved_id() {
                    self.request(WorkflowJob::Review(id));
                } else {
                    self.status = "先保存草稿再审阅".into();
                }
                return;
            }
            if key.code == KeyCode::F(8) {
                if self.capabilities.send
                    && self.view == WorkspaceView::Review
                    && let Some(r) = &self.review
                    && r.attachment_digests.len() == r.draft.input.attachments.len()
                {
                    self.dialog = Some(WorkspaceDialog::Approve(String::new()));
                } else {
                    self.status = "需要完整审阅与发送权限".into();
                }
                return;
            }
            if key.code == KeyCode::F(9) {
                if self.capabilities.send
                    && let Some(id) = self.saved_id()
                {
                    self.request(WorkflowJob::Send(id));
                } else {
                    self.status = "先保存并人工批准；发送权限不可由邮件改变".into();
                }
                return;
            }
            if key.code == KeyCode::Esc {
                if self.view == WorkspaceView::Review {
                    self.view = WorkspaceView::Editor;
                    return;
                }
                self.leave(false);
                self.browser.key(KeyCode::Esc);
                return;
            }
        }
        if self.view == WorkspaceView::Editor {
            if !self.capabilities.draft {
                if let Event::Key(key) = &event {
                    if key.code == KeyCode::Tab {
                        if let Some(e) = &mut self.editor {
                            e.focus = (e.focus + 1) % 5;
                        }
                    } else if key.code == KeyCode::BackTab
                        && let Some(e) = &mut self.editor
                    {
                        e.focus = (e.focus + 4) % 5;
                    }
                }
                self.status = "只读草稿；禁止编辑或附件路径操作".into();
                return;
            }
            if let Event::Key(key) = &event {
                match key.code {
                    KeyCode::Tab => {
                        if let Some(e) = &mut self.editor {
                            e.focus = (e.focus + 1) % 5;
                        }
                        return;
                    }
                    KeyCode::BackTab => {
                        if let Some(e) = &mut self.editor {
                            e.focus = (e.focus + 4) % 5;
                        }
                        return;
                    }
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.save(false);
                        return;
                    }
                    KeyCode::F(2) => {
                        if self.saved_id().is_some() {
                            self.dialog = Some(WorkspaceDialog::AddPath(String::new()));
                        } else {
                            self.status = "先保存再添加附件".into();
                        }
                        return;
                    }
                    KeyCode::F(3) => {
                        if self.saved_id().is_some() {
                            self.dialog = Some(WorkspaceDialog::Remove(String::new()));
                        }
                        return;
                    }
                    KeyCode::F(4) => {
                        if let Some(id) = self.saved_id() {
                            self.request(WorkflowJob::SyncDraft(id));
                        }
                        return;
                    }
                    _ => {}
                }
            }
            if let Some(e) = &mut self.editor
                && let Err(error) = e.edit(event)
            {
                self.status = error.into();
            }
            return;
        }
        if let Event::Key(key) = event {
            if self.view == WorkspaceView::Review {
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.scroll = (self.scroll + 1).min(
                            self.review_wrapped
                                .len()
                                .max(self.review_lines.len())
                                .saturating_sub(1),
                        )
                    }
                    KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                    KeyCode::PageDown => {
                        self.scroll = (self.scroll + 20).min(
                            self.review_wrapped
                                .len()
                                .max(self.review_lines.len())
                                .saturating_sub(1),
                        )
                    }
                    KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(20),
                    KeyCode::End => {
                        self.scroll = self
                            .review_wrapped
                            .len()
                            .max(self.review_lines.len())
                            .saturating_sub(1)
                    }
                    KeyCode::Home => self.scroll = 0,
                    KeyCode::Right => self.horizontal = self.horizontal.saturating_add(10),
                    KeyCode::Left => self.horizontal = self.horizontal.saturating_sub(10),
                    KeyCode::Char('q') => self.leave(true),
                    _ => {}
                }
                return;
            }
            if self.browser.search_edit && self.view == WorkspaceView::Mail {
                self.browser.key(key.code);
                return;
            }
            match key.code {
                KeyCode::Char('c') if self.capabilities.draft => {
                    self.generation = self.generation.wrapping_add(1);
                    self.editor = Some(DraftEditor::new(self.empty(), None));
                    self.review = None;
                    self.view = WorkspaceView::Editor;
                    self.status = "新建 · Tab 字段 · Ctrl+S 保存 · F6 审阅".into();
                }
                KeyCode::Char('d') => {
                    self.view = WorkspaceView::Drafts;
                    self.offset = 0;
                    self.request(WorkflowJob::Drafts(0));
                }
                KeyCode::Char('s') => {
                    self.view = WorkspaceView::Submissions;
                    self.offset = 0;
                    self.request(WorkflowJob::Submissions(0));
                }
                KeyCode::Char('R') | KeyCode::Char('A') | KeyCode::Char('F')
                    if self.view == WorkspaceView::Mail
                        && self.browser.ui.reading
                        && self.capabilities.draft =>
                {
                    if let Some(m) = self.browser.messages.get(self.browser.ui.selected) {
                        let id = m.id.clone();
                        if key.code == KeyCode::Char('F') {
                            self.dialog = Some(WorkspaceDialog::Forward(id, String::new()));
                        } else {
                            self.request(WorkflowJob::Reply(
                                id,
                                if key.code == KeyCode::Char('A') {
                                    ReplyMode::ReplyAll
                                } else {
                                    ReplyMode::Reply
                                },
                                vec![],
                            ));
                        }
                    }
                }
                KeyCode::Char('x')
                    if self.view == WorkspaceView::Mail && self.browser.ui.reading =>
                {
                    if let Some(m) = self.browser.messages.get(self.browser.ui.selected) {
                        self.dialog =
                            Some(WorkspaceDialog::ExportPart(m.id.clone(), String::new()));
                    }
                }
                KeyCode::Char('q') => self.leave(true),
                KeyCode::Down | KeyCode::Char('j') if self.view != WorkspaceView::Mail => {
                    let len = if self.view == WorkspaceView::Drafts {
                        self.drafts.len()
                    } else {
                        self.submissions.len()
                    };
                    self.selected = (self.selected + 1).min(len.saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') if self.view != WorkspaceView::Mail => {
                    self.selected = self.selected.saturating_sub(1)
                }
                KeyCode::Char('n') | KeyCode::Char('p') if self.view != WorkspaceView::Mail => {
                    self.offset = if key.code == KeyCode::Char('n') {
                        (self.offset + 50).min(100000)
                    } else {
                        self.offset.saturating_sub(50)
                    };
                    self.request(if self.view == WorkspaceView::Drafts {
                        WorkflowJob::Drafts(self.offset)
                    } else {
                        WorkflowJob::Submissions(self.offset)
                    });
                }
                KeyCode::Enter if self.view == WorkspaceView::Drafts => {
                    if let Some(d) = self.drafts.get(self.selected) {
                        self.request(WorkflowJob::Load(d.id));
                    }
                }
                KeyCode::Enter | KeyCode::Char('r') if self.view == WorkspaceView::Submissions => {
                    if let Some(s) = self.submissions.get(self.selected) {
                        self.request(WorkflowJob::Status(s.id));
                    }
                }
                KeyCode::Char('e') if self.view == WorkspaceView::Submissions => {
                    if let Some(s) = self.submissions.get(self.selected) {
                        self.request(WorkflowJob::Evidence(s.id));
                    }
                }
                KeyCode::Char('u') if self.view == WorkspaceView::Submissions => {
                    if let Some(s) = self.submissions.get(self.selected) {
                        self.request(WorkflowJob::Recover(s.id));
                    }
                }
                KeyCode::F(4) if self.view == WorkspaceView::Submissions => {
                    if let Some(s) = self.submissions.get(self.selected) {
                        self.request(WorkflowJob::SyncSent(s.id));
                    }
                }
                KeyCode::Char('v') => {
                    if let Some(id) = self.sync_id {
                        self.request(WorkflowJob::SyncStatus(id));
                    }
                }
                _ if self.view == WorkspaceView::Mail => {
                    self.browser.key(key.code);
                    self.quit = self.browser.ui.quit;
                }
                _ => {}
            }
        }
    }
    fn dialog_event(&mut self, event: Event) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let key = match &event {
            Event::Key(k) => Some(k.code),
            _ => None,
        };
        if key == Some(KeyCode::Esc) {
            return;
        }
        match dialog {
            WorkspaceDialog::Dirty { quit } => {
                match key {
                    Some(KeyCode::Char('s')) => {
                        if self.save(false) {
                            self.after_save = Some(quit);
                        } else {
                            self.dialog = Some(WorkspaceDialog::Dirty { quit });
                        }
                    }
                    Some(KeyCode::Char('d')) => {
                        self.editor = None;
                        self.review = None;
                        self.view = WorkspaceView::Mail;
                        self.quit = quit;
                        self.generation = self.generation.wrapping_add(1);
                    }
                    Some(KeyCode::Char('c')) => {}
                    _ => self.dialog = Some(dialog),
                }
                return;
            }
            WorkspaceDialog::Conflict => {
                match key {
                    Some(KeyCode::Char('n')) => {
                        self.save(true);
                    }
                    Some(KeyCode::Char('r')) => {
                        if let Some(id) = self
                            .editor
                            .as_ref()
                            .and_then(|e| e.saved.as_ref())
                            .map(|d| d.id)
                        {
                            self.request(WorkflowJob::Load(id));
                        }
                    }
                    Some(KeyCode::Char('c')) => {}
                    _ => self.dialog = Some(dialog),
                }
                return;
            }
            _ => {}
        }
        if key == Some(KeyCode::Enter) {
            match dialog {
                WorkspaceDialog::Approve(typed) => {
                    if let Some(r) = &self.review {
                        self.request(WorkflowJob::Approve(
                            r.draft.id,
                            r.draft.digest.clone(),
                            typed,
                        ));
                    }
                }
                WorkspaceDialog::AddPath(path) => {
                    if !path.is_empty() {
                        self.dialog = Some(WorkspaceDialog::AddType(
                            PathBuf::from(path),
                            "application/octet-stream".into(),
                        ));
                    }
                }
                WorkspaceDialog::AddType(path, kind) => {
                    if let Some(d) = self.editor.as_ref().and_then(|e| e.saved.as_ref()) {
                        self.request(WorkflowJob::Add(d.id, d.revision, path, kind));
                    }
                }
                WorkspaceDialog::Remove(index) => {
                    if let Ok(index) = index.parse()
                        && let Some(d) = self.editor.as_ref().and_then(|e| e.saved.as_ref())
                    {
                        self.request(WorkflowJob::Remove(d.id, d.revision, index));
                    } else {
                        self.status = "附件编号无效".into();
                    }
                }
                WorkspaceDialog::Forward(id, to) => {
                    self.request(WorkflowJob::Reply(
                        id,
                        ReplyMode::Forward,
                        to.split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                    ));
                }
                WorkspaceDialog::ExportPart(id, part) => {
                    if let Ok(part) = part.parse() {
                        self.dialog =
                            Some(WorkspaceDialog::ExportDirectory(id, part, String::new()));
                    } else {
                        self.status = "MIME part 编号无效".into();
                    }
                }
                WorkspaceDialog::ExportDirectory(id, part, path) => {
                    self.request(WorkflowJob::Export(id, part, PathBuf::from(path)));
                }
                _ => {}
            }
            return;
        }
        let text = match &mut dialog {
            WorkspaceDialog::Approve(t)
            | WorkspaceDialog::AddPath(t)
            | WorkspaceDialog::Remove(t) => t,
            WorkspaceDialog::AddType(_, t)
            | WorkspaceDialog::Forward(_, t)
            | WorkspaceDialog::ExportPart(_, t)
            | WorkspaceDialog::ExportDirectory(_, _, t) => t,
            _ => unreachable!(),
        };
        match event {
            Event::Key(k) => match k.code {
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(c)
                    if !c.is_control() && !dangerous(c) && text.len() + c.len_utf8() <= 4096 =>
                {
                    text.push(c)
                }
                _ => {}
            },
            Event::Paste(p)
                if !p.chars().any(|c| c.is_control() || dangerous(c))
                    && text.len() + p.len() <= 4096 =>
            {
                text.push_str(&p)
            }
            Event::Paste(_) => self.status = "拒绝危险或过大的对话框粘贴".into(),
            _ => {}
        }
        self.dialog = Some(dialog);
    }
}

fn submission_result(workspace: &dyn HumanWorkspace, info: SubmissionInfo) -> WorkflowData {
    // A failed diagnostic query cannot erase a known SMTP result or trigger a send.
    let diagnostic = if info.state_recorded {
        workspace.submission_diagnostic(info.id).ok().flatten()
    } else {
        None
    };
    WorkflowData::SubmissionDiagnostic(info, diagnostic)
}
fn execute_workflow(
    workspace: &dyn HumanWorkspace,
    job: &WorkflowJob,
) -> WorkflowResult<WorkflowData> {
    use WorkflowJob::*;
    match job {
        Drafts(offset) => workspace.list_drafts(*offset, 50).map(WorkflowData::Drafts),
        Load(id) => workspace
            .load_draft(*id)
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Create(input) => workspace
            .create(input.clone())
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Save(id, revision, input) => workspace
            .save(*id, *revision, input.clone())
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Review(id) => workspace
            .review(*id)
            .map(|d| WorkflowData::Review(Box::new(d))),
        Approve(id, digest, typed) => workspace
            .approve(*id, digest, typed)
            .map(WorkflowData::Approved),
        Send(id) => workspace
            .send(*id)
            .map(|info| submission_result(workspace, info)),
        Reply(id, mode, to) => workspace
            .reply(id, *mode, "", to)
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Add(id, revision, path, kind) => workspace
            .add_attachment(*id, *revision, path, kind)
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Remove(id, revision, index) => workspace
            .remove_attachment(*id, *revision, *index)
            .map(|d| WorkflowData::Draft(Box::new(d))),
        Export(id, part, path) => workspace
            .export_attachment(id, *part, path)
            .map(WorkflowData::Exported),
        Submissions(offset) => workspace
            .list_submissions(*offset, 50)
            .map(WorkflowData::Submissions),
        Status(id) => workspace
            .submission_status(*id)
            .map(|info| submission_result(workspace, info)),
        Evidence(id) => workspace.sent_evidence(*id).map(WorkflowData::Evidence),
        Recover(id) => workspace
            .recover_submission(*id)
            .map(|info| submission_result(workspace, info)),
        SyncDraft(id) => workspace.sync_draft(*id).map(WorkflowData::Sync),
        SyncSent(id) => workspace.sync_sent(*id).map(WorkflowData::Sync),
        SyncStatus(id) => workspace.sync_status(*id).map(WorkflowData::Sync),
    }
}
enum WorkerRequest {
    Workflow(u64, u64, Box<WorkflowJob>),
    Browser(u64, BrowserJob),
}
enum WorkerResponse {
    Workflow(Box<WorkflowResponse>),
    Browser(BrowserResponse),
}
struct Lane {
    requests: mpsc::SyncSender<WorkerRequest>,
    responses: mpsc::Receiver<WorkerResponse>,
}
impl Lane {
    fn new(host: Arc<dyn Host>, workspace: Arc<dyn HumanWorkspace>) -> Self {
        let (requests, rx) = mpsc::sync_channel(1);
        let (tx, responses) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                let response = match request {
                    WorkerRequest::Workflow(generation, edit_generation, job) => {
                        let job = *job;
                        let result = execute_workflow(workspace.as_ref(), &job);
                        WorkerResponse::Workflow(Box::new(WorkflowResponse {
                            generation,
                            edit_generation,
                            job,
                            result,
                        }))
                    }
                    WorkerRequest::Browser(generation, job) => {
                        let result = match job {
                            BrowserJob::Folders => match host.list_folders() {
                                Ok(f) => Ok(BrowserData::Folders(f)),
                                Err(e) if e.0 == "unsupported" => Ok(BrowserData::Folders(vec![])),
                                Err(e) => Err(e),
                            },
                            BrowserJob::List {
                                folder,
                                query,
                                offset,
                            } => if query.is_empty() {
                                host.list_folder_messages(folder.as_deref(), offset, 50)
                            } else {
                                host.search_messages(folder.as_deref(), &query, offset, 50)
                            }
                            .map(BrowserData::List),
                            BrowserJob::Read(id) => host
                                .read_message(&id)
                                .map(|d| BrowserData::Read(Box::new(d))),
                        };
                        WorkerResponse::Browser(BrowserResponse { generation, result })
                    }
                };
                // Mutation has already been persisted before the acknowledgement.
                // Receiver drop cannot cancel a claimed send or a committed save.
                if tx.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            responses,
        }
    }
}
/// A local lane and a network lane; no queue grows while a lane is busy.
/// Dropping the UI detaches workers; already acknowledged requests finish.
pub struct WorkspaceWorker {
    local: Lane,
    network: Lane,
}
impl WorkspaceWorker {
    pub fn new(host: Arc<dyn Host>, workspace: Arc<dyn HumanWorkspace>) -> Self {
        Self {
            local: Lane::new(host.clone(), workspace.clone()),
            network: Lane::new(host, workspace),
        }
    }
    pub fn poll(&mut self, state: &mut WorkspaceState) -> bool {
        let mut changed = false;
        for lane in [&self.local, &self.network] {
            if let Ok(response) = lane.responses.try_recv() {
                changed = true;
                match response {
                    WorkerResponse::Workflow(r) => state.complete(*r),
                    WorkerResponse::Browser(r) => {
                        state.network_busy = false;
                        state.browser.complete(r);
                    }
                }
            }
        }
        for (network, lane) in [(false, &self.local), (true, &self.network)] {
            if let Some((generation, edit_generation, job)) = state.take_request(network) {
                changed = true;
                if lane
                    .requests
                    .try_send(WorkerRequest::Workflow(
                        generation,
                        edit_generation,
                        Box::new(job),
                    ))
                    .is_err()
                {
                    if network {
                        state.network_busy = false;
                    } else {
                        state.local_busy = false;
                    }
                    state.status = "工作线程不可用；查询持久化记录，不重试发送".into();
                }
            }
        }
        if !state.network_busy
            && state.view == WorkspaceView::Mail
            && !state.quit
            && let Some((generation, job)) = state.browser.take_request()
        {
            state.network_busy = true;
            if self
                .network
                .requests
                .try_send(WorkerRequest::Browser(generation, job))
                .is_err()
            {
                state.network_busy = false;
                state.status = "网络线程不可用".into();
            }
        }
        changed
    }
}
fn block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}
fn escaped_document(state: &WorkspaceState) -> String {
    state
        .browser
        .document
        .as_ref()
        .map(|d| {
            format!(
                "{}\nFrom: {}\n\n{}\n\nAttachments:\n{}",
                inert_text(&d.subject, 256),
                inert_text(&d.from, 256),
                inert_text(&d.text, 65536),
                d.attachments
                    .iter()
                    .map(|a| format!(
                        "part {}: {} | {} | {} bytes",
                        a.id,
                        inert_text(&a.name, 256),
                        inert_text(&a.media_type, 256),
                        a.size
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .unwrap_or_else(|| state.browser.status.clone())
}
/// Wrap full review text by display cells, without slicing UTF-8 bytes.
/// Combining marks remain attached to the preceding scalar. No content is dropped.
pub fn wrap_review(lines: &[String], width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut result = Vec::new();
    for line in lines {
        let mut current = String::new();
        let mut used = 0;
        for c in line.chars() {
            let cells = ratatui::text::Span::raw(c.to_string()).width();
            if used + cells > width && !current.is_empty() {
                result.push(current);
                current = String::new();
                used = 0;
            }
            current.push(c);
            used += cells;
        }
        result.push(current);
    }
    result
}
pub fn render_workspace(frame: &mut Frame, state: &mut WorkspaceState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
        area,
    );
    if area.width == 0 || area.height == 0 {
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(if area.height >= 8 { 2 } else { 1 }),
        Constraint::Min(1),
        Constraint::Length(if area.height >= 8 { 3 } else { 1 }),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "LetterKeeper · {} · {}",
            review_escape(&state.capabilities.account),
            review_escape(&state.capabilities.from)
        ))
        .style(Style::default().fg(Color::Cyan)),
        sections[0],
    );
    let main = sections[1];
    match state.view {
        WorkspaceView::Editor => {
            if let Some(editor) = &mut state.editor {
                let labels = ["To", "Cc", "Bcc", "Subject", "Body"];
                if main.height >= 18 {
                    let fields = Layout::vertical([
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Min(3),
                    ])
                    .split(main);
                    for index in 0..5 {
                        editor.fields[index]
                            .set_block(block(labels[index], editor.focus == index).to_owned());
                        frame.render_widget(&editor.fields[index], fields[index]);
                    }
                } else {
                    editor.fields[editor.focus]
                        .set_block(block(labels[editor.focus], true).to_owned());
                    frame.render_widget(&editor.fields[editor.focus], main);
                }
            }
        }
        WorkspaceView::Review => {
            // Cache reflow until width or snapshot changes; bounded by backend body budget.
            let width = main.width.saturating_sub(2).max(1) as usize;
            if state.review_width != width {
                state.review_wrapped = wrap_review(&state.review_lines, width);
                state.review_width = width;
                state.scroll = state
                    .scroll
                    .min(state.review_wrapped.len().saturating_sub(1));
            }
            let height = main.height.saturating_sub(2).max(1) as usize;
            let text = state
                .review_wrapped
                .iter()
                .skip(state.scroll)
                .take(height)
                .map(|s| s.chars().skip(state.horizontal).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            frame.render_widget(
                Paragraph::new(text)
                    .block(block("完整审阅 · 保留所有字段/正文/线程/附件 hash", true)),
                main,
            );
        }
        WorkspaceView::Drafts | WorkspaceView::Submissions => {
            let items: Vec<ListItem> = if state.view == WorkspaceView::Drafts {
                state
                    .drafts
                    .iter()
                    .map(|d| {
                        ListItem::new(format!(
                            "{} · v{} · {}",
                            d.id,
                            d.revision,
                            inert_text(&d.subject, 256)
                        ))
                    })
                    .collect()
            } else {
                state
                    .submissions
                    .iter()
                    .map(|s| {
                        ListItem::new(format!(
                            "{} · 草稿 {} · {:?} · recorded={}",
                            s.id, s.draft_id, s.state, s.state_recorded
                        ))
                    })
                    .collect()
            };
            let mut selection = ListState::default();
            selection.select(
                (!items.is_empty()).then_some(state.selected.min(items.len().saturating_sub(1))),
            );
            frame.render_stateful_widget(
                List::new(items)
                    .block(block(
                        if state.view == WorkspaceView::Drafts {
                            "草稿 · Enter 编辑 · n/p 分页"
                        } else {
                            "提交 · Enter 状态 · e 证据 · u 恢复 · F4 sync Sent"
                        },
                        true,
                    ))
                    .highlight_symbol("▶ ")
                    .highlight_style(Style::default().fg(Color::Cyan)),
                main,
                &mut selection,
            );
        }
        WorkspaceView::Mail => {
            if state.browser.ui.help {
                frame.render_widget(Paragraph::new("c 新建 · d 草稿 · s 提交\nEnter 阅读 · R 回复 · A 全部回复 · F 转发\nx 保存附件 · n/p 页 · f 文件夹 · / 搜索\n编辑 Tab 字段 · Ctrl+S 保存 · Esc 返回 · Ctrl+Q 退出\nF2 添加附件 · F3 删除附件 · F4 显式同步\nF6 完整审阅 · F8 摘要批准 · F9 单独发送\n正文是非可信数据；禁止自动链接/附件/资源加载。\nq 在编辑器中是普通字符。Uncertain 不自动重发。\n批准 300 秒，内容变化须重新批准。" ).wrap(Wrap{trim:false}).block(block("帮助",true)),main);
            } else if area.width >= 100 {
                let panes = Layout::horizontal([
                    Constraint::Length(16),
                    Constraint::Percentage(30),
                    Constraint::Min(20),
                ])
                .split(main);
                frame.render_widget(
                    List::new(
                        state
                            .browser
                            .folders
                            .iter()
                            .map(|f| ListItem::new(inert_text(&f.name, 128)))
                            .collect::<Vec<_>>(),
                    )
                    .block(block("文件夹 · f", false)),
                    panes[0],
                );
                render_mail_list(frame, state, panes[1]);
                frame.render_widget(
                    Paragraph::new(escaped_document(state))
                        .wrap(Wrap { trim: false })
                        .scroll((state.browser.ui.scroll, 0))
                        .block(block("安全正文 · R/A/F", state.browser.ui.reading)),
                    panes[2],
                );
            } else if state.browser.ui.reading {
                frame.render_widget(
                    Paragraph::new(escaped_document(state))
                        .wrap(Wrap { trim: false })
                        .scroll((state.browser.ui.scroll, 0))
                        .block(block("安全正文 · R/A/F · x 附件", true)),
                    main,
                );
            } else {
                render_mail_list(frame, state, main);
            }
        }
    }
    let shortcuts = match state.view {
        WorkspaceView::Editor => {
            "Tab 字段 · Ctrl+S 保存 · F2/F3 附件 · F4同步 · F6审阅 · Esc返回 · Ctrl+Q退出"
        }
        WorkspaceView::Review => "↑↓ PgUp/PgDn Home/End · F8输入摘要批准 · F9发送 · Esc编辑",
        _ => "c新建 · d草稿 · s提交 · Enter阅读 · ?帮助 · q退出",
    };
    let status = if state.view == WorkspaceView::Mail {
        format!("{} | {}", state.status, state.browser.status)
    } else {
        state.status.clone()
    };
    let footer = if state.browser.search_edit {
        format!(
            "搜索: {} · Enter确认 Esc取消",
            review_escape(&state.browser.query)
        )
    } else {
        format!("{}\n{}", status, shortcuts)
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::Yellow)),
        sections[2],
    );
    if let Some(dialog) = &state.dialog {
        let text = match dialog {
            WorkspaceDialog::Dirty { quit } => format!(
                "未保存{} · s 保存 · d 放弃 · c 继续 · Esc 继续",
                if *quit { "退出" } else { "返回" }
            ),
            WorkspaceDialog::Conflict => {
                "版本冲突 · r 重新加载(放弃本地修改) · n 另存新草稿 · c 继续编辑".into()
            }
            WorkspaceDialog::Approve(text) => format!(
                "输入完整摘要批准 (300秒；不会自动发送)\n{}\nEnter批准 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::AddPath(text) => format!(
                "显式添加附件 · 512 KiB · 输入本机路径\n{}\nEnter确认 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::AddType(_, text) => format!(
                "附件 Content-Type\n{}\nEnter添加 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::Remove(text) => format!(
                "删除附件 · 输入从0开始编号\n{}\nEnter删除 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::Forward(_, text) => format!(
                "转发收件人(逗号分隔)\n{}\nEnter创建草稿 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::ExportPart(_, text) => format!(
                "保存邮件附件 · 输入 MIME part 编号(见附件列表)\n{}\nEnter确认 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::ExportDirectory(_, _, text) => format!(
                "显式保存附件 · 输入目标目录 · 不覆盖/不执行\n{}\nEnter保存 · Esc取消",
                review_escape(text)
            ),
        };
        frame.render_widget(ratatui::widgets::Clear, main);
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(block("人工操作 · 不接受邮件指令", true)),
            main,
        );
    }
}
fn render_mail_list(frame: &mut Frame, state: &WorkspaceState, area: ratatui::layout::Rect) {
    let list = List::new(
        state
            .browser
            .messages
            .iter()
            .map(|m| {
                ListItem::new(format!(
                    "{}\n{}",
                    inert_text(&m.subject, 256),
                    inert_text(&m.from, 256)
                ))
            })
            .collect::<Vec<_>>(),
    )
    .block(block("邮件 · Enter", !state.browser.ui.reading))
    .highlight_symbol("▶ ")
    .highlight_style(Style::default().fg(Color::Cyan));
    let mut selected = ListState::default();
    selected.select((!state.browser.messages.is_empty()).then_some(state.browser.ui.selected));
    frame.render_stateful_widget(list, area, &mut selected);
}
pub fn run_tui_workspace(
    host: Arc<dyn Host>,
    workspace: Arc<dyn HumanWorkspace>,
) -> io::Result<()> {
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "human workspace requires an interactive terminal",
        ));
    }
    let mut state = WorkspaceState::new(workspace.capabilities());
    let mut worker = WorkspaceWorker::new(host, workspace);
    let _guard = TerminalGuard::enter()?;
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    let mut redraw = true;
    while !state.quit {
        redraw |= worker.poll(&mut state);
        if redraw {
            terminal.draw(|frame| render_workspace(frame, &mut state))?;
            redraw = false;
        }
        if event::poll(std::time::Duration::from_millis(20))? {
            state.event(event::read()?);
            redraw = true;
        }
    }
    Ok(())
}
