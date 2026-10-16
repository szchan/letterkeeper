// SPDX-License-Identifier: GPL-3.0-only
use super::*;

impl RemoteHost {
    pub(super) fn append_sync(
        &self,
        record: SyncRecord,
        raw: Vec<u8>,
        flags: &[String],
        client: ImapClient,
    ) -> Result<Value, HostError> {
        let result = if raw.len() > 2 * 1024 * 1024 {
            SendOutcome::Failed
        } else {
            outcome(client.append_message(&record.folder, &raw, flags))
        };
        match self.store()?.finish_sync(record.id, result) {
            Ok(done) => Ok(sync_view(&done)),
            Err(_) => Ok(json!({"id":record.id,"state":"Uncertain","state_recorded":false})),
        }
    }
    pub(super) fn workspace_sent_evidence(&self, id: i64) -> WorkflowResult<SentEvidence> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        let value =
            Host::sent_evidence(self, &id.to_string()).map_err(crate::workflow::host_error)?;
        let matching_messages = value["matches"]
            .as_array()
            .ok_or(WorkflowError::Unavailable)?
            .len();
        Ok(SentEvidence {
            submission_id: id,
            matching_messages,
        })
    }
    pub(super) fn workspace_sync_draft(&self, id: i64) -> WorkflowResult<SyncInfo> {
        if !self.config.permissions.draft {
            return Err(WorkflowError::Denied);
        }
        let value = Host::sync_draft(self, &id.to_string()).map_err(crate::workflow::host_error)?;
        let record_id = value["id"].as_i64().ok_or(WorkflowError::Unavailable)?;
        HumanWorkspace::sync_status(self, record_id)
    }
    pub(super) fn workspace_sync_sent(&self, id: i64) -> WorkflowResult<SyncInfo> {
        if !self.config.permissions.send {
            return Err(WorkflowError::Denied);
        }
        let value = Host::sync_sent(self, &id.to_string()).map_err(crate::workflow::host_error)?;
        let record_id = value["id"].as_i64().ok_or(WorkflowError::Unavailable)?;
        HumanWorkspace::sync_status(self, record_id)
    }
    pub(super) fn workspace_sync_status(&self, id: i64) -> WorkflowResult<SyncInfo> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_sync(id)
            .map(crate::workflow::sync_info)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn host_sync_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.draft_allowed()?;
        let draft_id = id(draft_id)?;
        let draft = self.store()?.get_draft(draft_id).map_err(err)?;
        validate_identity(&self.config, &draft.input).map_err(err)?;
        // Local credential/CA failures must not consume a remote-write claim.
        let client = self.imap()?;
        let record = self
            .store()?
            .claim_draft_sync(draft_id, &self.config.drafts_folder)
            .map_err(err)?;
        let sub = Submission {
            id: 0,
            draft_id: record.local_id,
            revision: record.revision,
            digest: record.digest.clone(),
            message_id: record.message_id.clone(),
            input: record.input.clone(),
            state: SubmissionState::Submitting,
        };
        let raw = match crate::transport::prepare_snapshot(&sub, true) {
            Ok(message) => message.formatted(),
            Err(_) => {
                return Ok(sync_view(
                    &self
                        .store()?
                        .finish_sync(record.id, SendOutcome::Failed)
                        .map_err(err)?,
                ));
            }
        };
        self.append_sync(record, raw, &["\\Draft".into()], client)
    }
    pub(super) fn host_sync_sent(&self, submission_id: &str) -> Result<Value, HostError> {
        self.send_allowed()?;
        if self.config.sent_copy == SentCopyStrategy::ServerManaged {
            return Err(err("sent_copy_is_server_managed_use_sent_evidence"));
        }
        let submission_id = id(submission_id)?;
        let sub = self.store()?.get_submission(submission_id).map_err(err)?;
        if sub.state != SubmissionState::Accepted {
            return Err(err("smtp_acceptance_required"));
        }
        let client = self.imap()?;
        let (record, raw) = {
            let mut store = self.store()?;
            let id = submission_id;
            let raw = store.submission_wire(id).map_err(err)?;
            let record = store
                .claim_sent_sync(id, &self.config.sent_folder)
                .map_err(err)?;
            (record, raw)
        };
        self.append_sync(record, raw, &["\\Seen".into()], client)
    }
    pub(super) fn host_sync_status(&self, record_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        Ok(sync_view(
            &self.store()?.get_sync(id(record_id)?).map_err(err)?,
        ))
    }
    pub(super) fn host_sent_evidence(&self, submission_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        if !self
            .config
            .allowed_folders
            .contains(&self.config.sent_folder)
        {
            return Err(err("folder_scope_denied"));
        }
        let sub = self
            .store()?
            .get_submission(id(submission_id)?)
            .map_err(err)?;
        let matches = self
            .imap()?
            .lookup_message_id(&self.config.sent_folder, &sub.message_id)
            .map_err(err)?
            .into_iter()
            .map(|k| self.message_id(&self.config.sent_folder, k.uid_validity, k.uid))
            .collect::<Vec<_>>();
        Ok(
            json!({"submission_id":sub.id,"message_id":sub.message_id,"smtp_state":sub.state,"matches":matches,"evidence_only":true,"not_found_does_not_mean_failed":true,"delivery_confirmed":false}),
        )
    }
}
