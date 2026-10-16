// SPDX-License-Identifier: GPL-3.0-only
use super::*;

impl RemoteHost {
    pub(super) fn folder(&self, folder: Option<&str>) -> Result<String, HostError> {
        let folder = match folder {
            Some(key) => self
                .config
                .allowed_folders
                .iter()
                .find(|name| folder_id(&self.config.id, name) == key),
            None => self
                .config
                .allowed_folders
                .iter()
                .find(|name| name.eq_ignore_ascii_case("INBOX"))
                .or(self.config.allowed_folders.first()),
        }
        .ok_or_else(|| err("folder_scope_denied"))?;
        Ok(folder.clone())
    }
    pub(super) fn message_key(&self, key: &str) -> Result<(String, u32, u32), HostError> {
        if key.len() > 128 {
            return Err(err("invalid_message_id"));
        }
        let parts = key.split('-').collect::<Vec<_>>();
        if parts.len() != 4 || parts[0] != "r" {
            return Err(err("invalid_message_id"));
        }
        let folder = self.folder(Some(&format!("f-{}", parts[1])))?;
        let validity = parts[2]
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| err("invalid_message_id"))?;
        let uid = parts[3]
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| err("invalid_message_id"))?;
        Ok((folder, validity, uid))
    }
    pub(super) fn message_id(&self, folder: &str, validity: u32, uid: u32) -> String {
        format!(
            "r-{}-{validity}-{uid}",
            &folder_id(&self.config.id, folder)[2..]
        )
    }
    pub(super) fn page(
        &self,
        folder: Option<&str>,
        query: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.read_allowed()?;
        let folder = self.folder(folder)?;
        let started = Instant::now();
        let page = self
            .imap()?
            .list_messages(&folder, offset, limit, query)
            .map_err(err)?;
        page.messages
            .into_iter()
            .map(|m| {
                if started.elapsed() > Duration::from_secs(30) {
                    return Err(err("mail_page_timeout"));
                }
                let doc = self.sandbox.parse(&m.headers).map_err(err)?;
                Ok(MessageSummary {
                    id: self.message_id(&folder, page.uid_validity, m.uid),
                    subject: doc.subject,
                    from: doc.from,
                })
            })
            .collect()
    }
    pub(super) fn raw(&self, key: &str) -> Result<Vec<u8>, HostError> {
        self.read_allowed()?;
        let (folder, validity, uid) = self.message_key(key)?;
        self.imap()?
            .read_message(&folder, validity, uid)
            .map_err(err)
    }
    pub fn inspect(&self, key: &str) -> Result<InspectedMail, HostError> {
        self.sandbox.inspect(&self.raw(key)?).map_err(err)
    }
    pub fn attachment(&self, key: &str, part: usize) -> Result<DecodedAttachment, HostError> {
        self.sandbox.attachment(&self.raw(key)?, part).map_err(err)
    }
    pub fn mark_seen(&self, key: &str) -> Result<(), HostError> {
        self.draft_allowed()?;
        let (folder, validity, uid) = self.message_key(key)?;
        self.imap()?.add_seen(&folder, validity, uid).map_err(err)
    }
    pub(super) fn workspace_export_attachment(
        &self,
        message_id: &str,
        part: usize,
        directory: &Path,
    ) -> WorkflowResult<String> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        let attachment = self
            .attachment(message_id, part)
            .map_err(|_| WorkflowError::Unavailable)?;
        crate::files::export_attachment(directory, &attachment.name, &attachment.bytes)
            .map_err(|_| WorkflowError::InvalidInput)?;
        Ok(attachment.name)
    }
    pub(super) fn host_list_messages(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.page(None, None, offset, limit)
    }
    pub(super) fn host_list_folder_messages(
        &self,
        folder: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.page(folder, None, offset, limit)
    }
    pub(super) fn host_search_messages(
        &self,
        folder: Option<&str>,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, HostError> {
        self.page(folder, Some(query), offset, limit)
    }
    pub(super) fn host_read_message(&self, key: &str) -> Result<SafeDocument, HostError> {
        self.sandbox.parse(&self.raw(key)?).map_err(err)
    }
    pub(super) fn host_list_accounts(&self) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(json!({"accounts":[{"id":self.config.id,"email":self.config.email}]}))
    }
    pub(super) fn host_list_folders(&self) -> Result<Vec<FolderSummary>, HostError> {
        self.read_allowed()?;
        Ok(self
            .imap()?
            .list_folders()
            .map_err(err)?
            .into_iter()
            .filter(|f| self.config.allowed_folders.contains(&f.name))
            .take(50)
            .map(|f| FolderSummary {
                id: folder_id(&self.config.id, &f.name),
                name: f.name,
            })
            .collect())
    }
}
