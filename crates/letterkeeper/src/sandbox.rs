// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_content::SafeDocument;
use std::path::PathBuf;

pub struct Sandbox {
    pub executable: PathBuf,
    pub launcher: PathBuf,
}

impl Sandbox {
    pub fn parse(&self, _raw: &[u8]) -> Result<SafeDocument, String> {
        Err("sandbox_not_implemented".into())
    }
}
