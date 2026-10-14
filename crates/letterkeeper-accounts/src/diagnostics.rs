// SPDX-License-Identifier: GPL-3.0-only
//! Read-only environment diagnostics; no credential items are inspected.
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageStatus {
    Ready,
    NotInitialized,
    Unsafe,
}

/// Inspect descriptors without creating directories or reading their contents.
pub fn inspect_private_directory(_path: &Path) -> StorageStatus {
    StorageStatus::Unsafe
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn directory_diagnostic_is_read_only_and_distinguishes_missing_unsafe_ready() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let missing = temp.path().join("not-initialized/nested");
        assert_eq!(
            inspect_private_directory(&missing),
            StorageStatus::NotInitialized
        );
        assert!(!temp.path().join("not-initialized").exists());
        assert_eq!(inspect_private_directory(temp.path()), StorageStatus::Ready);
        let link = temp.path().join("linked-parent");
        symlink(temp.path(), &link).unwrap();
        assert_eq!(
            inspect_private_directory(&link.join("missing")),
            StorageStatus::Unsafe
        );
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            inspect_private_directory(temp.path()),
            StorageStatus::Unsafe
        );
        assert_eq!(
            inspect_private_directory(Path::new("relative")),
            StorageStatus::Unsafe
        );
    }
}
