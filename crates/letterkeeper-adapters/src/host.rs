// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_content::SafeDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub subject: String,
    pub from: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftInput {
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
}
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HostError(pub String);
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSummary {
    pub id: String,
    pub name: String,
}
pub trait Host: Send + Sync {
    fn list_accounts(&self) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        if folder.is_none() {
            self.list_messages(offset, limit)
        } else {
            Err(HostError("unsupported".into()))
        }
    }
    fn search_messages(
        &self,
        _: Option<&str>,
        _: &str,
        _: usize,
        _: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn reply_draft(&self, _: &str, _: &str, _: &str, _: &[String]) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn sync_draft(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn sync_sent(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn sync_status(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }
    fn sent_evidence(&self, _: &str) -> Result<Value, HostError> {
        Err(HostError("unsupported".into()))
    }

    fn list_messages(&self, offset: usize, limit: usize) -> Result<Vec<MessageSummary>, HostError>;
    fn read_message(&self, id: &str) -> Result<SafeDocument, HostError>;
    fn create_draft(&self, input: DraftInput) -> Result<Value, HostError>;
    fn get_draft(&self, id: &str) -> Result<Value, HostError>;
    fn send_draft(&self, id: &str) -> Result<Value, HostError>;
    fn send_status(&self, id: &str) -> Result<Value, HostError>;
}
