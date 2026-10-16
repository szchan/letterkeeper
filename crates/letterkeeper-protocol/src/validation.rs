// SPDX-License-Identifier: GPL-3.0-only
use crate::{Credentials, ProtocolError};
pub(crate) fn valid(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
pub(crate) fn quoted(value: &str) -> Result<String, ProtocolError> {
    if !valid(value, 1024) {
        return Err(ProtocolError::InvalidInput);
    }
    Ok(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}
pub(crate) fn credentials_valid(c: &Credentials) -> bool {
    valid(&c.username, 320) && valid(&c.secret, 16 * 1024)
}
