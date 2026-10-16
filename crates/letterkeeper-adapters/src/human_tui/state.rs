// SPDX-License-Identifier: GPL-3.0-only
use super::*;
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
    pub(super) pending_local: Option<WorkflowJob>,
    pub(super) pending_network: Option<WorkflowJob>,
    pub(super) after_save: Option<bool>,
}
impl WorkspaceState {
    pub(super) fn show_submission(
        &mut self,
        info: SubmissionInfo,
        diagnostic: WorkflowResult<Option<SendDiagnostic>>,
    ) {
        if let Some(old) = self.submissions.iter_mut().find(|s| s.id == info.id) {
            *old = info.clone();
        } else {
            self.submissions.insert(0, info.clone());
            self.submissions.truncate(50);
        }
        let (detail, code) = if !info.state_recorded {
            ("unknown（状态未落盘）".into(), None)
        } else {
            match diagnostic {
                Ok(Some(d)) => {
                    // Serde names are fixed enum codes, never server strings.
                    let stage = serde_json::to_value(d.stage).expect("enum serializes");
                    let kind = serde_json::to_value(d.kind).expect("enum serializes");
                    (
                        format!(
                            "{} / {}",
                            stage.as_str().unwrap_or("unknown"),
                            kind.as_str().unwrap_or("unknown")
                        ),
                        d.smtp_code,
                    )
                }
                Ok(None) if info.state == letterkeeper_core::SubmissionState::Accepted => {
                    ("无失败诊断（仅 SMTP 接受）".into(), None)
                }
                _ => ("unknown（旧记录或诊断不可用）".into(), None),
            }
        };
        let code = code
            .map(|code| format!("SMTP {code} · "))
            .unwrap_or_default();
        self.status = format!(
            "提交 {} {:?} · durable={}\n{}\n{}不自动重试 · s 查看记录",
            info.id, info.state, info.state_recorded, detail, code
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
    pub(super) fn empty(&self) -> CoreInput {
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
            Ok(WorkflowData::Submission(info)) => self.show_submission(info, Ok(None)),
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
    pub(super) fn save(&mut self, as_new: bool) -> bool {
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
    pub(super) fn saved_id(&self) -> Option<i64> {
        self.editor
            .as_ref()
            .filter(|e| !e.dirty)
            .and_then(|e| e.saved.as_ref())
            .map(|d| d.id)
    }
    pub(super) fn leave(&mut self, quit: bool) {
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
}
