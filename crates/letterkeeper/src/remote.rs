// SPDX-License-Identifier: GPL-3.0-only
use crate::sandbox::Sandbox;
use letterkeeper_accounts::{AccountConfig,CredentialStore};
use letterkeeper_adapters::{Host,HostError,MessageSummary,DraftInput,FolderSummary};
use letterkeeper_content::SafeDocument;
use serde_json::Value;
use std::{path::Path,sync::Arc};
pub struct RemoteHost;
impl RemoteHost {
    pub fn open(_config:AccountConfig,_state_dir:&Path,_vault:Arc<dyn CredentialStore>,_sandbox:Sandbox)->Result<Self,String>{Err("remote_host_not_implemented".into())}
}
impl Host for RemoteHost {
    fn list_messages(&self,_offset:usize,_limit:usize)->Result<Vec<MessageSummary>,HostError>{Err(HostError("not implemented".into()))}
    fn read_message(&self,_id:&str)->Result<SafeDocument,HostError>{Err(HostError("not implemented".into()))}
    fn create_draft(&self,_input:DraftInput)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn get_draft(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn send_draft(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn send_status(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn list_folders(&self)->Result<Vec<FolderSummary>,HostError>{Err(HostError("not implemented".into()))}
    fn reply_draft(&self,_id:&str,_mode:&str,_body:&str,_to:&[String])->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn sync_draft(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn sync_sent(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
    fn sent_evidence(&self,_id:&str)->Result<Value,HostError>{Err(HostError("not implemented".into()))}
}
