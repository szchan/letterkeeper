// SPDX-License-Identifier: GPL-3.0-only
//! Fixed-field, sanitized doctor reports.
use letterkeeper_accounts::diagnostics::{StorageStatus, inspect_private_directory};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Ready,
    Unavailable,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub config_storage: StorageStatus,
    pub data_storage: StorageStatus,
    pub sandbox: SandboxStatus,
}

pub fn diagnose(config_dir: &Path, data_dir: &Path) -> DoctorReport {
    DoctorReport {
        config_storage: inspect_private_directory(config_dir),
        data_storage: inspect_private_directory(data_dir),
        sandbox: SandboxStatus::Unavailable,
    }
}
