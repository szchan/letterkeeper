// SPDX-License-Identifier: GPL-3.0-only
use super::*;

impl RemoteHost {
    pub fn create_full(&self, input: DraftInput) -> Result<Draft, HostError> {
        self.draft_allowed()?;
        validate_identity(&self.config, &input).map_err(err)?;
        self.store()?.create_draft(input).map_err(err)
    }
    pub fn update_full(
        &self,
        draft_id: i64,
        revision: i64,
        input: DraftInput,
    ) -> Result<Draft, HostError> {
        self.draft_allowed()?;
        validate_identity(&self.config, &input).map_err(err)?;
        self.store()?
            .update_draft(draft_id, revision, input)
            .map_err(err)
    }
    pub(super) fn workspace_list_drafts(
        &self,
        offset: usize,
        limit: usize,
    ) -> WorkflowResult<Vec<DraftSummary>> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        Ok(self
            .store()
            .map_err(|_| WorkflowError::Unavailable)?
            .list_drafts(offset, limit)
            .map_err(crate::workflow::core_error)?
            .into_iter()
            .map(crate::workflow::draft_summary)
            .collect())
    }
    pub(super) fn workspace_load_draft(&self, id: i64) -> WorkflowResult<Draft> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_draft(id)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn workspace_create(&self, input: DraftInput) -> WorkflowResult<Draft> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        validate_identity(&self.config, &input).map_err(|_| WorkflowError::Denied)?;
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .create_draft(input)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn workspace_save(
        &self,
        id: i64,
        expected_revision: i64,
        input: DraftInput,
    ) -> WorkflowResult<Draft> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        validate_identity(&self.config, &input).map_err(|_| WorkflowError::Denied)?;
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .update_draft(id, expected_revision, input)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn workspace_reply(
        &self,
        message_id: &str,
        mode: ReplyMode,
        body: &str,
        to: &[String],
    ) -> WorkflowResult<Draft> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        let mode = match mode {
            ReplyMode::Reply => "reply",
            ReplyMode::ReplyAll => "reply_all",
            ReplyMode::Forward => "forward",
        };
        let value = Host::reply_draft(self, message_id, mode, body, to)
            .map_err(crate::workflow::host_error)?;
        let id = value["id"].as_i64().ok_or(WorkflowError::Unavailable)?;
        self.load_draft(id)
    }
    pub(super) fn workspace_add_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        path: &Path,
        content_type: &str,
    ) -> WorkflowResult<Draft> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        let mut draft = self.load_draft(id)?;
        validate_identity(&self.config, &draft.input).map_err(|_| WorkflowError::Denied)?;
        if draft.revision != expected_revision {
            return Err(WorkflowError::Conflict);
        }
        let attachment = crate::files::import_attachment(path, content_type)
            .map_err(|_| WorkflowError::InvalidInput)?;
        draft.input.attachments.push(attachment);
        self.save(id, expected_revision, draft.input)
    }
    pub(super) fn workspace_remove_attachment(
        &self,
        id: i64,
        expected_revision: i64,
        index: usize,
    ) -> WorkflowResult<Draft> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        let mut draft = self.load_draft(id)?;
        validate_identity(&self.config, &draft.input).map_err(|_| WorkflowError::Denied)?;
        if draft.revision != expected_revision {
            return Err(WorkflowError::Conflict);
        }
        if index >= draft.input.attachments.len() {
            return Err(WorkflowError::InvalidInput);
        }
        draft.input.attachments.remove(index);
        self.save(id, expected_revision, draft.input)
    }
    pub(super) fn workspace_review(&self, id: i64) -> WorkflowResult<ReviewSnapshot> {
        Ok(crate::workflow::review_snapshot(self.load_draft(id)?))
    }
    pub(super) fn workspace_approve(
        &self,
        id: i64,
        expected_digest: &str,
        typed_digest: &str,
    ) -> WorkflowResult<ApprovalInfo> {
        if !self.config.permissions.send {
            return Err(WorkflowError::Denied);
        }
        let mut store = self.store().map_err(|_| WorkflowError::Unavailable)?;
        let d = store.get_draft(id).map_err(crate::workflow::core_error)?;
        validate_identity(&self.config, &d.input).map_err(|_| WorkflowError::Denied)?;
        crate::workflow::approve(&mut store, id, expected_digest, typed_digest)
    }
    pub(super) fn host_create_draft(&self, input: AdapterDraft) -> Result<Value, HostError> {
        Ok(draft_view(&self.create_full(DraftInput {
            account: self.config.id.clone(),
            from: self.config.email.clone(),
            to: input.to,
            cc: vec![],
            bcc: vec![],
            subject: input.subject,
            body: input.body,
            attachments: vec![],
            in_reply_to: None,
            references: vec![],
        })?))
    }
    pub(super) fn host_get_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(draft_view(
            &self.store()?.get_draft(id(draft_id)?).map_err(err)?,
        ))
    }
    pub(super) fn host_reply_draft(
        &self,
        key: &str,
        mode: &str,
        body: &str,
        to: &[String],
    ) -> Result<Value, HostError> {
        self.draft_allowed()?;
        let kind = match mode {
            "reply" => ComposeKind::Reply,
            "reply_all" => ComposeKind::ReplyAll,
            "forward" => ComposeKind::Forward,
            _ => return Err(err("invalid_reply_mode")),
        };
        if (kind == ComposeKind::Forward && to.is_empty())
            || (kind != ComposeKind::Forward && !to.is_empty())
        {
            return Err(err("invalid_forward_recipients"));
        }
        let original = self.inspect(key)?;
        let mut draft = compose(&original, &self.config.email, kind, body).map_err(err)?;
        if kind == ComposeKind::Forward {
            draft.to = to.to_vec();
        }
        Ok(draft_view(&self.create_full(DraftInput {
            account: self.config.id.clone(),
            from: self.config.email.clone(),
            to: draft.to,
            cc: draft.cc,
            bcc: vec![],
            subject: draft.subject,
            body: draft.body,
            attachments: vec![],
            in_reply_to: draft.in_reply_to,
            references: draft.references,
        })?))
    }
}
