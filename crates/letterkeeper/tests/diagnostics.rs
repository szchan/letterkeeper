// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper::{
    diagnostics::{SandboxStatus, inspect_sandbox},
    sandbox::Sandbox,
};
use std::path::PathBuf;

#[test]
fn doctor_requires_a_functional_isolated_worker_not_a_present_launcher() {
    let mut sandbox = Sandbox {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_letterkeeper")),
        launcher: PathBuf::from("/usr/bin/bwrap"),
    };
    assert_eq!(inspect_sandbox(&sandbox), SandboxStatus::Ready);
    sandbox.launcher = PathBuf::from("/usr/bin/false");
    assert_eq!(inspect_sandbox(&sandbox), SandboxStatus::Unavailable);
    sandbox.launcher = PathBuf::from("/missing-doctor-launcher");
    assert_eq!(inspect_sandbox(&sandbox), SandboxStatus::Unavailable);
    sandbox.launcher = PathBuf::from("/usr/bin/bwrap");
    sandbox.executable = PathBuf::from("/usr/bin/false");
    assert_eq!(inspect_sandbox(&sandbox), SandboxStatus::Unavailable);
}
