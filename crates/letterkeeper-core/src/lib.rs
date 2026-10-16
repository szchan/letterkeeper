// SPDX-License-Identifier: GPL-3.0-only
//! Shared draft, authorization and submission state.
mod error;
mod permissions;
mod store;
mod types;
mod validation;

pub use error::{Error, Result};
pub use permissions::{AccessPolicy, Permissions};
pub use store::Store;
pub use types::{
    Attachment, Draft, DraftInput, DraftListEntry, SendDiagnostic, SendFailureKind, SendOutcome,
    SendStage, Submission, SubmissionListEntry, SubmissionState, SyncKind, SyncRecord, SyncState,
};
