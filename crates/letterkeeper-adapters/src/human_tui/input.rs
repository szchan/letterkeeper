// SPDX-License-Identifier: GPL-3.0-only
use super::*;
impl WorkspaceState {
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
    pub(super) fn dialog_event(&mut self, event: Event) {
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
