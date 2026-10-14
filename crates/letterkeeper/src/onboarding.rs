// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_accounts::{
    AccountConfig, OnboardingEndpoint, OnboardingInput, OnboardingProvider, OnboardingScope,
    SentCopyStrategy,
};
use std::io::{BufRead, Read, Write};

const MAX_LINE_BYTES: usize = 1024;
struct Wizard<'a, R, W> {
    input: &'a mut R,
    output: &'a mut W,
}
impl<R: BufRead, W: Write> Wizard<'_, R, W> {
    fn line(&mut self, prompt: &str) -> Result<Option<String>, String> {
        write!(self.output, "{prompt}: ").map_err(|_| "output_failed")?;
        self.output.flush().map_err(|_| "output_failed")?;
        let mut bytes = Vec::new();
        self.input
            .by_ref()
            .take((MAX_LINE_BYTES + 2) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "terminal_read_failed")?;
        if bytes.len() > MAX_LINE_BYTES + 1 {
            // Abort the entire wizard; remaining bytes cannot become confirmation.
            return Err("account_input_too_long".into());
        }
        if bytes.last() != Some(&b'\n') {
            return Ok(None);
        }
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let value = String::from_utf8(bytes).map_err(|_| "invalid_account_input")?;
        if value.chars().any(|c| {
            c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        }) {
            return Err("invalid_account_input".into());
        }
        if value.trim() == "/cancel" {
            return Ok(None);
        }
        Ok(Some(value))
    }
}
fn optional(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}
fn port(value: String) -> Result<Option<u16>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        value
            .parse()
            .map(Some)
            .map_err(|_| "invalid_account_port".into())
    }
}

/// A configuration-only line wizard. The caller checks TTY before calling and
/// opens storage only after Some(config). No secrets, credentials or networking.
pub fn collect(
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Option<AccountConfig>, String> {
    let mut wizard = Wizard { input, output };
    writeln!(wizard.output, "LetterKeeper 账户配置 / account setup\nEnter /cancel at any prompt to cancel. No authentication or network connection occurs.\nDo not enter passwords, tokens or client secrets here.")
        .map_err(|_| "output_failed")?;
    macro_rules! line {
        ($prompt:expr) => {
            match wizard.line($prompt)? {
                Some(value) => value.trim().to_owned(),
                None => return Ok(None),
            }
        };
    }
    let id = line!("Account ID (lowercase letters/digits/-/_)");
    let email = line!("Email address");
    let provider = line!("Authentication [password/gmail; default password]");
    let username = optional(line!("Username [default email]"));
    let provider = match provider.as_str() {
        "" | "password" => {
            let imap = OnboardingEndpoint {
                host: line!("IMAP host (implicit TLS)"),
                port: port(line!("IMAP port [993]"))?,
            };
            let smtp = OnboardingEndpoint {
                host: line!("SMTP host (implicit TLS)"),
                port: port(line!("SMTP port [465]"))?,
            };
            let strategy = line!("Sent copy [server_managed/append; default server_managed]");
            let sent_copy = match strategy.as_str() {
                "" | "server_managed" => SentCopyStrategy::ServerManaged,
                "append" => SentCopyStrategy::Append,
                _ => return Err("invalid_sent_copy_choice".into()),
            };
            OnboardingProvider::Password {
                imap,
                smtp,
                sent_copy,
            }
        }
        "gmail" => {
            let client_id = line!("Your own Google Desktop OAuth client ID (not a secret)");
            let client_secret_ref =
                match line!("Client secret reference needed? [yes/no; default no]").as_str() {
                    "yes" => true,
                    "" | "no" => false,
                    _ => return Err("invalid_client_secret_reference_choice".into()),
                };
            OnboardingProvider::Gmail {
                client_id,
                client_secret_ref,
            }
        }
        _ => return Err("invalid_authentication_choice".into()),
    };
    let folders = line!("Allowed folders, comma-separated [INBOX]");
    let allowed_folders = if folders.is_empty() {
        vec!["INBOX".into()]
    } else {
        folders.split(',').map(|s| s.trim().to_owned()).collect()
    };
    let drafts_folder = optional(line!("Drafts folder [provider default]"));
    let sent_folder = optional(line!("Sent folder [provider default]"));
    let scope = match line!(
        "Permissions [read_only/draft/send; default read_only; send still requires human approval]"
    )
    .as_str()
    {
        "" | "read_only" => OnboardingScope::ReadOnly,
        "draft" => OnboardingScope::Draft,
        "send" => OnboardingScope::Send,
        _ => return Err("invalid_permissions_choice".into()),
    };
    let cfg = OnboardingInput {
        id,
        email,
        username,
        allowed_folders,
        scope,
        drafts_folder,
        sent_folder,
        provider,
    }
    .build()
    .map_err(|_| "invalid_account_configuration")?;
    // Builder validation precedes echoing any user-derived configuration. This
    // complete review contains no secrets and no private filesystem paths.
    writeln!(
        wizard.output,
        "\nReview configuration (authentication: unverified):"
    )
    .map_err(|_| "output_failed")?;
    serde_json::to_writer_pretty(&mut *wizard.output, &cfg).map_err(|_| "output_failed")?;
    writeln!(wizard.output).map_err(|_| "output_failed")?;
    match wizard.line("Save this new account configuration? Type yes [default no]")? {
        Some(value) if value == "yes" => Ok(Some(cfg)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn truncated_overlong_and_terminal_controls_cannot_confirm() {
        for value in ["demo", "demo\u{202e}\n", "demo\u{1b}[31m\n"] {
            assert!(
                collect(&mut Cursor::new(value.as_bytes()), &mut Vec::new())
                    .ok()
                    .flatten()
                    .is_none()
            );
        }
        let long = format!("{}\nyes\n", "a".repeat(MAX_LINE_BYTES + 1));
        assert_eq!(
            collect(&mut Cursor::new(long), &mut Vec::new()).unwrap_err(),
            "account_input_too_long"
        );
    }
}
