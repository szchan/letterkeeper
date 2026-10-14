// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_accounts::AccountConfig;
use std::io::{BufRead, Write};
/// Collect configuration before storage exists; no authentication occurs here.
pub fn collect(
    _input: &mut impl BufRead,
    _output: &mut impl Write,
) -> Result<Option<AccountConfig>, String> {
    Ok(None)
}
