// SPDX-License-Identifier: GPL-3.0-only
use crate::*;
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
        }
    }
}
impl BrowserState {
    pub fn take_request(&mut self) -> Option<(u64, BrowserJob)> {
        None
    }
    pub fn complete(&mut self, _: BrowserResponse) {}
    pub fn key(&mut self, k: KeyCode) {
        self.ui.key(k, self.messages.len())
    }
}
pub fn render_dynamic(frame: &mut Frame, state: &BrowserState) {
    render(frame, &state.ui, &state.messages, &[])
}
