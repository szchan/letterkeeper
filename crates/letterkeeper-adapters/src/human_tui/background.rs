// SPDX-License-Identifier: GPL-3.0-only
use super::*;
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
    pub(super) fn network(&self) -> bool {
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
    pub(super) fn readonly(&self) -> bool {
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
    SubmissionDiagnostic(SubmissionInfo, WorkflowResult<Option<SendDiagnostic>>),
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
fn submission_result(workspace: &dyn HumanWorkspace, info: SubmissionInfo) -> WorkflowData {
    // A failed diagnostic query cannot erase a known SMTP result or trigger a send.
    let diagnostic = if info.state_recorded {
        workspace.submission_diagnostic(info.id)
    } else {
        Err(WorkflowError::Unavailable)
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
    pub(super) fn new(host: Arc<dyn Host>, workspace: Arc<dyn HumanWorkspace>) -> Self {
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
