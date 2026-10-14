// SPDX-License-Identifier: GPL-3.0-only
//! Fixed-field, sanitized doctor reports.
use crate::sandbox::Sandbox;
use letterkeeper_accounts::diagnostics::{
    CredentialServiceStatus, StorageStatus, inspect_credential_service, inspect_private_directory,
    inspect_runtime_directory,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationStatus {
    NotChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorAdvice {
    InitializeAccountStorage,
    RepairPrivateStorage,
    StartPrivateLoginSession,
    EnableSandboxNamespaces,
    StartCredentialService,
    InitializeDefaultCollection,
    UnlockCredentialCollectionManually,
    CheckLocalSessionBus,
    RetryCredentialService,
    AccountAuthenticationNotChecked,
}

impl DoctorAdvice {
    pub fn message(self) -> &'static str {
        match self {
            Self::InitializeAccountStorage => {
                "Run letterkeeper account add or account import to initialize private storage."
            }
            Self::RepairPrivateStorage => {
                "Check private directory ownership, 0700 permissions, and symbolic links."
            }
            Self::StartPrivateLoginSession => {
                "Use a login session with a private runtime directory."
            }
            Self::EnableSandboxNamespaces => {
                "Check bubblewrap installation and user/network namespace permissions."
            }
            Self::StartCredentialService => {
                "Check that busctl and the session credential service are available."
            }
            Self::InitializeDefaultCollection => {
                "Initialize a default collection manually in the credential manager."
            }
            Self::UnlockCredentialCollectionManually => {
                "Unlock the default collection manually in the credential manager."
            }
            Self::CheckLocalSessionBus => {
                "Check the local session bus and credential service metadata."
            }
            Self::RetryCredentialService => {
                "The credential metadata check timed out; check the service and retry."
            }
            Self::AccountAuthenticationNotChecked => {
                "Account credentials and provider authentication were not checked."
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub config_storage: StorageStatus,
    pub data_storage: StorageStatus,
    pub runtime_storage: StorageStatus,
    pub sandbox: SandboxStatus,
    pub credential_service: CredentialServiceStatus,
    pub authentication: AuthenticationStatus,
    pub environment_ready: bool,
    pub advice: Vec<DoctorAdvice>,
    pub hints: Vec<&'static str>,
}

impl DoctorReport {
    /// Environment readiness is not evidence of account login or Gmail compatibility.
    pub fn is_ready(&self) -> bool {
        self.environment_ready
    }
}

pub fn diagnose(config_dir: &Path, data_dir: &Path) -> DoctorReport {
    let sandbox = Sandbox::current().map_or(SandboxStatus::Unavailable, |sandbox| {
        inspect_sandbox(&sandbox)
    });
    assemble_report(
        inspect_private_directory(config_dir),
        inspect_private_directory(data_dir),
        inspect_runtime_directory(),
        sandbox,
        inspect_credential_service(),
    )
}

/// Execute the real isolated worker on synthetic mail; executable presence alone
/// is insufficient. Full HTTP/DNS/host-file isolation remains a CI acceptance gate.
pub fn inspect_sandbox(sandbox: &Sandbox) -> SandboxStatus {
    const SYNTHETIC: &[u8] = b"Subject: doctor\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nletterkeeper-doctor-synthetic";
    match sandbox.parse(SYNTHETIC) {
        Ok(document)
            if document.subject == "doctor"
                && document.text.trim() == "letterkeeper-doctor-synthetic" =>
        {
            SandboxStatus::Ready
        }
        _ => SandboxStatus::Unavailable,
    }
}

fn assemble_report(
    config_storage: StorageStatus,
    data_storage: StorageStatus,
    runtime_storage: StorageStatus,
    sandbox: SandboxStatus,
    credential_service: CredentialServiceStatus,
) -> DoctorReport {
    let mut advice = Vec::new();
    if [config_storage, data_storage].contains(&StorageStatus::NotInitialized) {
        advice.push(DoctorAdvice::InitializeAccountStorage);
    }
    if [config_storage, data_storage, runtime_storage].contains(&StorageStatus::Unsafe) {
        advice.push(DoctorAdvice::RepairPrivateStorage);
    }
    if runtime_storage != StorageStatus::Ready {
        advice.push(DoctorAdvice::StartPrivateLoginSession);
    }
    if sandbox != SandboxStatus::Ready {
        advice.push(DoctorAdvice::EnableSandboxNamespaces);
    }
    match credential_service {
        CredentialServiceStatus::Unlocked => (),
        CredentialServiceStatus::Locked => {
            advice.push(DoctorAdvice::UnlockCredentialCollectionManually)
        }
        CredentialServiceStatus::NotInitialized => {
            advice.push(DoctorAdvice::InitializeDefaultCollection)
        }
        CredentialServiceStatus::Unavailable => advice.push(DoctorAdvice::StartCredentialService),
        CredentialServiceStatus::Unknown => advice.push(DoctorAdvice::CheckLocalSessionBus),
        CredentialServiceStatus::TimedOut => advice.push(DoctorAdvice::RetryCredentialService),
    }
    advice.push(DoctorAdvice::AccountAuthenticationNotChecked);
    DoctorReport {
        config_storage,
        data_storage,
        runtime_storage,
        sandbox,
        credential_service,
        authentication: AuthenticationStatus::NotChecked,
        environment_ready: [config_storage, data_storage, runtime_storage]
            .iter()
            .all(|status| *status == StorageStatus::Ready)
            && sandbox == SandboxStatus::Ready
            && credential_service == CredentialServiceStatus::Unlocked,
        hints: advice.iter().map(|advice| advice.message()).collect(),
        advice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_separates_environment_readiness_from_authentication_and_fixed_output() {
        let ready = assemble_report(
            StorageStatus::Ready,
            StorageStatus::Ready,
            StorageStatus::Ready,
            SandboxStatus::Ready,
            CredentialServiceStatus::Unlocked,
        );
        assert!(ready.is_ready());
        assert_eq!(ready.authentication, AuthenticationStatus::NotChecked);
        assert_eq!(
            ready.advice,
            vec![DoctorAdvice::AccountAuthenticationNotChecked]
        );
        for credential in [
            CredentialServiceStatus::Unknown,
            CredentialServiceStatus::Locked,
            CredentialServiceStatus::Unavailable,
            CredentialServiceStatus::NotInitialized,
            CredentialServiceStatus::TimedOut,
        ] {
            let report = assemble_report(
                StorageStatus::Ready,
                StorageStatus::Ready,
                StorageStatus::Ready,
                SandboxStatus::Ready,
                credential,
            );
            assert!(!report.is_ready());
            let json = serde_json::to_value(report).unwrap();
            let keys: Vec<_> = json
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                keys,
                [
                    "advice",
                    "authentication",
                    "config_storage",
                    "credential_service",
                    "data_storage",
                    "environment_ready",
                    "hints",
                    "runtime_storage",
                    "sandbox"
                ]
            );
        }
        let denied = assemble_report(
            StorageStatus::Unsafe,
            StorageStatus::NotInitialized,
            StorageStatus::NotInitialized,
            SandboxStatus::Unavailable,
            CredentialServiceStatus::Unknown,
        );
        assert!(!denied.is_ready());
        assert!(
            denied
                .advice
                .contains(&DoctorAdvice::InitializeAccountStorage)
        );
        assert!(denied.advice.contains(&DoctorAdvice::RepairPrivateStorage));
        assert!(
            denied
                .advice
                .contains(&DoctorAdvice::EnableSandboxNamespaces)
        );
    }
}
