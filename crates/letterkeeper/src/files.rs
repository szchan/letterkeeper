// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_core::Attachment;
use std::path::Path;
pub fn import_attachment(_path: &Path, _content_type: &str) -> Result<Attachment,String> {
    Err("attachment_not_implemented".into())
}
pub fn export_attachment(_directory:&Path,_name:&str,_bytes:&[u8])->Result<(),String> {
    Err("attachment_not_implemented".into())
}
