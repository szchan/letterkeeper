// SPDX-License-Identifier: GPL-3.0-only
use std::fmt;
#[derive(Clone, Debug)]
pub struct Endpoint { pub host: String, pub port: u16 }
#[derive(Clone, Debug, Default)]
pub struct TlsOptions { pub roots_pem: Option<Vec<u8>> }
#[derive(Clone)]
pub struct Credentials { pub username: String, pub secret: String, pub oauth: bool }
impl fmt::Debug for Credentials { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { f.write_str("Credentials([redacted])") } }
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError { #[error("invalid protocol input")] InvalidInput, #[error("protocol connection failed")] Connection, #[error("protocol operation failed")] Rejected, #[error("protocol response exceeds limit")] Limit, #[error("protocol operation timed out")] Timeout, #[error("mailbox identity changed")] StaleMailbox, #[error("message not found")] NotFound }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder { pub name: String, pub attributes: Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMessage { pub uid: u32, pub headers: Vec<u8>, pub size: u32, pub flags: Vec<String> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePage { pub uid_validity: u32, pub messages: Vec<RemoteMessage>, pub more: bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteKey { pub uid_validity: u32, pub uid: u32 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome { Accepted, Failed, Uncertain }
pub struct ImapClient { endpoint: Endpoint, tls: TlsOptions, credentials: Credentials }
impl ImapClient {
 pub fn new(endpoint:Endpoint,tls:TlsOptions,credentials:Credentials)->Self { Self{endpoint,tls,credentials} }
 pub fn list_folders(&self)->Result<Vec<Folder>,ProtocolError> { Err(ProtocolError::Rejected) }
 pub fn list_messages(&self,_folder:&str,_offset:usize,_limit:usize,_query:Option<&str>)->Result<MessagePage,ProtocolError> { Err(ProtocolError::Rejected) }
 pub fn read_message(&self,_folder:&str,_uid_validity:u32,_uid:u32)->Result<Vec<u8>,ProtocolError> { Err(ProtocolError::Rejected) }
 pub fn lookup_message_id(&self,_folder:&str,_message_id:&str)->Result<Vec<RemoteKey>,ProtocolError> { Err(ProtocolError::Rejected) }
 pub fn add_seen(&self,_folder:&str,_uid_validity:u32,_uid:u32)->Result<(),ProtocolError> { Err(ProtocolError::Rejected) }
 pub fn append_message(&self,_folder:&str,_raw:&[u8],_flags:&[String])->WriteOutcome { WriteOutcome::Failed }
}
pub fn send_smtp(_endpoint:&Endpoint,_tls:&TlsOptions,_credentials:&Credentials,_from:&str,_to:&[String],_raw:&[u8])->WriteOutcome { WriteOutcome::Failed }
