// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, Result};
use std::path::Path;
#[cfg(unix)]
pub(super) fn private_path(path: &Path) -> Result<()> {
    use std::{
        fs::{self, DirBuilder, OpenOptions},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(Error::UnsafePath)?;
    if !parent.exists() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|_| Error::UnsafePath)?;
    }
    let metadata = fs::symlink_metadata(parent).map_err(|_| Error::UnsafePath)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(Error::UnsafePath);
    }
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.nlink() == 1 && m.permissions().mode() & 0o777 == 0o600 => Ok(()),
        Ok(_) => Err(Error::UnsafePath),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
            {
                Ok(_) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => private_path(path),
                Err(_) => Err(Error::UnsafePath),
            }
        }
        Err(_) => Err(Error::UnsafePath),
    }
}
#[cfg(not(unix))]
pub(super) fn private_path(_: &Path) -> Result<()> {
    Err(Error::UnsafePath)
}
