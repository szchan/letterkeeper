// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::{SendOutcome, Submission};
use std::net::SocketAddr;
pub fn prepare(_submission: &Submission) -> Result<lettre::Message, String> {
    Err("not_implemented".into())
}
pub fn send_test(_submission: &Submission, _endpoint: SocketAddr) -> SendOutcome {
    SendOutcome::Failed
}
