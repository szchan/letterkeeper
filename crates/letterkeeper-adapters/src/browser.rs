// SPDX-License-Identifier: GPL-3.0-only
use crate::mcp::{id_check, query_check};
use crate::{FolderSummary, Host, HostError, MessageSummary, UiState, inert_text, render};
use crossterm::event::KeyCode;
use letterkeeper_content::SafeDocument;
use ratatui::{Frame, widgets::Paragraph};
use std::sync::Arc;
#[derive(Debug, Clone)]
pub enum BrowserJob {
    Folders,
    List {
        folder: Option<String>,
        query: String,
        offset: usize,
    },
    Read(String),
}
pub enum BrowserData {
    Folders(Vec<FolderSummary>),
    List(Vec<MessageSummary>),
    Read(Box<SafeDocument>),
}
pub struct BrowserResponse {
    pub generation: u64,
    pub result: Result<BrowserData, HostError>,
}
pub struct BrowserState {
    pub ui: UiState,
    pub messages: Vec<MessageSummary>,
    pub document: Option<SafeDocument>,
    pub folders: Vec<FolderSummary>,
    pub folder: Option<String>,
    pub offset: usize,
    pub query: String,
    pub search_edit: bool,
    pub status: String,
    pub generation: u64,
    pub busy: bool,
    pending: Option<BrowserJob>,
    flight: Option<u64>,
}
impl Default for BrowserState {
    fn default() -> Self {
        Self {
            ui: UiState::default(),
            messages: vec![],
            document: None,
            folders: vec![],
            folder: None,
            offset: 0,
            query: String::new(),
            search_edit: false,
            status: String::new(),
            generation: 0,
            busy: false,
            pending: Some(BrowserJob::Folders),
            flight: None,
        }
    }
}
impl BrowserState {
    fn list_job(&self) -> BrowserJob {
        BrowserJob::List {
            folder: self.folder.clone(),
            query: self.query.clone(),
            offset: self.offset,
        }
    }
    fn request(&mut self, job: BrowserJob) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = Some(job);
        self.document = None;
        self.status = "加载中 · Esc 取消 · q 退出".into();
    }
    pub fn take_request(&mut self) -> Option<(u64, BrowserJob)> {
        if self.busy || self.ui.quit {
            return None;
        }
        let job = self.pending.take()?;
        self.busy = true;
        self.flight = Some(self.generation);
        self.status = "加载中 · Esc 取消 · q 退出".into();
        Some((self.generation, job))
    }
    pub fn complete(&mut self, response: BrowserResponse) {
        if self.flight != Some(response.generation) {
            return;
        }
        self.busy = false;
        self.flight = None;
        if response.generation != self.generation {
            return;
        }
        match response.result {
            Ok(BrowserData::Folders(folders)) => {
                self.folders = folders
                    .into_iter()
                    .filter(|f| id_check(&f.id).is_ok())
                    .take(50)
                    .collect();
                self.folder = self.folders.first().map(|f| f.id.clone());
                self.pending = Some(self.list_job());
            }
            Ok(BrowserData::List(messages)) => {
                self.messages = messages
                    .into_iter()
                    .filter(|m| id_check(&m.id).is_ok())
                    .take(50)
                    .collect();
                self.ui.selected = 0;
                self.status = if self.messages.is_empty() {
                    "没有邮件"
                } else {
                    "就绪"
                }
                .into();
            }
            Ok(BrowserData::Read(document)) => {
                self.document = Some(*document);
                self.status = "就绪".into();
            }
            Err(_) => {
                self.status = "操作失败；r 重试 · Esc 返回 · q 退出".into();
            }
        }
    }
    pub fn key(&mut self, key: KeyCode) {
        if self.search_edit {
            match key {
                KeyCode::Esc => {
                    self.search_edit = false;
                    self.query.clear();
                    self.key(KeyCode::Esc);
                    self.status = "搜索已取消".into();
                }
                KeyCode::Enter => {
                    self.search_edit = false;
                    if query_check(&self.query).is_ok() {
                        self.offset = 0;
                        self.ui.reading = false;
                        self.request(self.list_job());
                    } else {
                        self.status = "搜索不能为空或超出限制".into();
                    }
                }
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char(c) if !c.is_control() && self.query.len() + c.len_utf8() <= 1024 => {
                    self.query.push(c)
                }
                _ => {}
            }
            return;
        }
        match key {
            KeyCode::Esc => {
                self.generation = self.generation.wrapping_add(1);
                self.pending = None;
                self.ui.reading = false;
                self.document = None;
                self.status = if self.busy {
                    "已取消显示；等待原请求结束"
                } else {
                    "已返回"
                }
                .into();
            }
            KeyCode::Char('r') => {
                self.ui.reading = false;
                self.request(if self.folders.is_empty() && self.messages.is_empty() {
                    BrowserJob::Folders
                } else {
                    self.list_job()
                });
            }
            KeyCode::Char('n') if !self.ui.reading && self.messages.len() == 50 => {
                self.offset = (self.offset + 50).min(100000);
                self.request(self.list_job());
            }
            KeyCode::Char('p') if !self.ui.reading => {
                self.offset = self.offset.saturating_sub(50);
                self.request(self.list_job());
            }
            KeyCode::Char('f') if !self.folders.is_empty() => {
                let current = self
                    .folders
                    .iter()
                    .position(|f| Some(&f.id) == self.folder.as_ref())
                    .unwrap_or(0);
                self.folder = Some(self.folders[(current + 1) % self.folders.len()].id.clone());
                self.offset = 0;
                self.query.clear();
                self.ui.reading = false;
                self.request(self.list_job());
            }
            KeyCode::Char('/') => {
                self.search_edit = true;
                self.query.clear();
            }
            KeyCode::Enter | KeyCode::Tab if !self.ui.help => {
                if self.ui.reading {
                    self.key(KeyCode::Esc);
                } else if let Some(message) = self.messages.get(self.ui.selected) {
                    let id = message.id.clone();
                    self.ui.reading = true;
                    self.ui.scroll = 0;
                    self.request(BrowserJob::Read(id));
                }
            }
            _ => self.ui.key(key, self.messages.len()),
        }
    }
}
/// One worker per TUI session, bounded request/result channels and no queued
/// network work while one task is in flight. Cancellation invalidates output;
/// host protocol deadlines terminate underlying operations. Drop never joins it.
pub struct BrowserWorker {
    requests: std::sync::mpsc::SyncSender<(u64, BrowserJob)>,
    responses: std::sync::mpsc::Receiver<BrowserResponse>,
}
impl BrowserWorker {
    pub fn new(host: Arc<dyn Host>) -> Self {
        let (requests, rx) = std::sync::mpsc::sync_channel::<(u64, BrowserJob)>(1);
        let (tx, responses) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok((generation, job)) = rx.recv() {
                let result = match job {
                    BrowserJob::Folders => match host.list_folders() {
                        Ok(folders) => Ok(BrowserData::Folders(folders)),
                        // Older offline hosts use their existing list_messages API.
                        Err(error) if error.0 == "unsupported" => Ok(BrowserData::Folders(vec![])),
                        Err(error) => Err(error),
                    },
                    BrowserJob::List {
                        folder,
                        query,
                        offset,
                    } => {
                        let result = if query.is_empty() {
                            host.list_folder_messages(folder.as_deref(), offset, 50)
                        } else {
                            host.search_messages(folder.as_deref(), &query, offset, 50)
                        };
                        result.map(BrowserData::List)
                    }
                    BrowserJob::Read(id) => host
                        .read_message(&id)
                        .map(|d| BrowserData::Read(Box::new(d))),
                };
                if tx.send(BrowserResponse { generation, result }).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            responses,
        }
    }
    pub fn poll(&mut self, state: &mut BrowserState) {
        if let Ok(response) = self.responses.try_recv() {
            state.complete(response);
        }
        if let Some(request) = state.take_request()
            && self.requests.try_send(request).is_err()
        {
            state.busy = false;
            state.flight = None;
            state.status = "工作线程不可用".into();
        }
    }
}
pub fn render_dynamic(frame: &mut Frame, state: &BrowserState) {
    let view = UiState {
        selected: if state.ui.reading {
            0
        } else {
            state.ui.selected
        },
        reading: state.ui.reading,
        quit: state.ui.quit,
        help: state.ui.help,
        scroll: state.ui.scroll,
    };
    let documents = state.document.as_slice();
    render(frame, &view, &state.messages, documents);
    let area = frame.area();
    if area.height > 0 {
        let folder = state
            .folders
            .iter()
            .find(|f| Some(&f.id) == state.folder.as_ref())
            .map(|f| inert_text(&f.name, 40))
            .unwrap_or_else(|| "默认文件夹".into());
        let status = if state.search_edit {
            format!(
                "搜索：{} · Enter 确认 · Esc 取消",
                inert_text(&state.query, 100)
            )
        } else {
            format!(
                "{} · {} · 第 {} 页 · r 刷新 n/p 分页 f 文件夹 / 搜索",
                state.status,
                folder,
                state.offset / 50 + 1
            )
        };
        frame.render_widget(
            Paragraph::new(status),
            ratatui::layout::Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );
    }
}
