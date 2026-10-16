// SPDX-License-Identifier: GPL-3.0-only
use super::*;

impl RemoteHost {
    pub(super) fn finish_send(
        &self,
        sub: &Submission,
        outcome: SendOutcome,
        diagnostic: Option<SendDiagnostic>,
    ) -> Value {
        // Preserve the known network result only when the state and diagnostic commit together.
        let done = self.store().and_then(|mut store| {
            store
                .finish_submission_with_diagnostic(sub.id, outcome, diagnostic)
                .map_err(err)
        });
        match done {
            Ok(done) => {
                let mut value = submission_view(&done);
                value["diagnostic"] = json!(diagnostic);
                value
            }
            Err(_) => {
                json!({"id":sub.id,"draft_id":sub.draft_id,"state":"Uncertain","state_recorded":false,"diagnostic":null})
            }
        }
    }
    pub fn recover_sender(&self, submission_id: i64) -> Result<Value, HostError> {
        self.send_allowed()?;
        let _lock = private_lock(&self.state_dir, "send.lock", Duration::ZERO).map_err(err)?;
        Ok(submission_view(
            &self
                .store()?
                .recover_interrupted(submission_id)
                .map_err(err)?,
        ))
    }
    pub(super) fn workspace_send(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        if !self.config.permissions.send {
            return Err(WorkflowError::Denied);
        }
        let value = Host::send_draft(self, &id.to_string()).map_err(crate::workflow::host_error)?;
        let sub_id = value["id"].as_i64().ok_or(WorkflowError::Unavailable)?;
        let state = match value["state"].as_str() {
            Some("Accepted") => SubmissionState::Accepted,
            Some("Failed") => SubmissionState::Failed,
            _ => SubmissionState::Uncertain,
        };
        Ok(SubmissionInfo {
            id: sub_id,
            draft_id: id,
            state,
            state_recorded: value["state_recorded"].as_bool() == Some(true),
        })
    }
    pub(super) fn workspace_list_submissions(
        &self,
        offset: usize,
        limit: usize,
    ) -> WorkflowResult<Vec<SubmissionInfo>> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        Ok(self
            .store()
            .map_err(|_| WorkflowError::Unavailable)?
            .list_submissions(offset, limit)
            .map_err(crate::workflow::core_error)?
            .into_iter()
            .map(crate::workflow::submission_entry_info)
            .collect())
    }
    pub(super) fn workspace_submission_status(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_submission(id)
            .map(crate::workflow::submission_info)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn workspace_submission_diagnostic(
        &self,
        id: i64,
    ) -> WorkflowResult<Option<SendDiagnostic>> {
        if !self.config.permissions.read {
            return Err(WorkflowError::Denied);
        }
        self.store()
            .map_err(|_| WorkflowError::Unavailable)?
            .get_submission_diagnostic(id)
            .map_err(crate::workflow::core_error)
    }
    pub(super) fn workspace_recover_submission(&self, id: i64) -> WorkflowResult<SubmissionInfo> {
        if !self.config.permissions.send {
            return Err(WorkflowError::Denied);
        }
        self.recover_sender(id)
            .map_err(crate::workflow::host_error)?;
        self.submission_status(id)
    }
    pub(super) fn host_send_draft(&self, draft_id: &str) -> Result<Value, HostError> {
        self.send_allowed()?;
        let _sender = private_lock(&self.state_dir, "send.lock", Duration::ZERO).map_err(err)?;
        let sub = {
            let mut store = self.store()?;
            let draft = store.get_draft(id(draft_id)?).map_err(err)?;
            validate_identity(&self.config, &draft.input).map_err(err)?;
            store
                .claim_send(draft.id, now().map_err(err)?)
                .map_err(err)?
        };
        let raw = match crate::transport::prepare(&sub) {
            Ok(message) => message.formatted(),
            Err(_) => {
                return Ok(self.finish_send(
                    &sub,
                    SendOutcome::Failed,
                    Some(local_diagnostic(
                        SendStage::Preparation,
                        SendFailureKind::InvalidInput,
                    )),
                ));
            }
        };
        if raw.len() > 2 * 1024 * 1024 {
            return Ok(self.finish_send(
                &sub,
                SendOutcome::Failed,
                Some(local_diagnostic(
                    SendStage::Preparation,
                    SendFailureKind::InvalidInput,
                )),
            ));
        }
        let recorded = self
            .store()
            .and_then(|mut store| store.record_submission_wire(sub.id, &raw).map_err(err));
        if recorded.is_err() {
            return Ok(self.finish_send(
                &sub,
                SendOutcome::Failed,
                Some(local_diagnostic(
                    SendStage::Preparation,
                    SendFailureKind::Unavailable,
                )),
            ));
        }
        let to = sub
            .input
            .to
            .iter()
            .chain(&sub.input.cc)
            .chain(&sub.input.bcc)
            .cloned()
            .collect::<Vec<_>>();
        let tls = match self.tls() {
            Ok(tls) => tls,
            Err(_) => {
                return Ok(self.finish_send(
                    &sub,
                    SendOutcome::Failed,
                    Some(local_diagnostic(
                        SendStage::TlsConfiguration,
                        SendFailureKind::Unavailable,
                    )),
                ));
            }
        };
        if let Err(error) =
            letterkeeper_protocol::validate_smtp_tls(&endpoint(&self.config.smtp), &tls)
        {
            let kind = match error {
                ProtocolError::InvalidInput => SendFailureKind::InvalidInput,
                _ => SendFailureKind::Unavailable,
            };
            return Ok(self.finish_send(
                &sub,
                SendOutcome::Failed,
                Some(local_diagnostic(SendStage::TlsConfiguration, kind)),
            ));
        }
        let credentials = match self.credentials() {
            Ok(credentials) => credentials,
            Err(_) => {
                return Ok(self.finish_send(
                    &sub,
                    SendOutcome::Failed,
                    Some(local_diagnostic(
                        SendStage::Credentials,
                        SendFailureKind::Unavailable,
                    )),
                ));
            }
        };
        let report = letterkeeper_protocol::send_smtp_report(
            &endpoint(&self.config.smtp),
            &tls,
            &credentials,
            &sub.input.from,
            &to,
            &raw,
        );
        Ok(self.finish_send(
            &sub,
            outcome(report.outcome),
            report.diagnostic.map(diagnostic),
        ))
    }
    pub(super) fn host_send_status(&self, submission_id: &str) -> Result<Value, HostError> {
        self.read_allowed()?;
        let store = self.store()?;
        let submission_id = id(submission_id)?;
        let sub = store.get_submission(submission_id).map_err(err)?;
        Ok(submission_status_view(&store, &sub))
    }
}
