// SPDX-License-Identifier: GPL-3.0-only
use super::*;

impl RemoteHost {
    pub(super) fn credentials(&self) -> Result<Credentials, HostError> {
        let _lock =
            credential_lock(self.vault.as_ref(), &self.state_dir, &self.config.id).map_err(err)?;
        let auth = match self.config.authentication {
            Authentication::Password => password_auth_material(&self.config, self.vault.as_ref()),
            Authentication::GmailOAuth { .. } => GmailOAuth::new(&self.config, self.vault.as_ref())
                .and_then(|g| g.auth_material(unix_now()?)),
        }
        .map_err(err)?;
        Ok(Credentials {
            username: auth.username,
            secret: auth.secret.expose().to_owned(),
            oauth: auth.oauth,
        })
    }
    pub(super) fn tls(&self) -> Result<TlsOptions, HostError> {
        let roots_pem = self
            .config
            .extra_ca_pem
            .as_ref()
            .map(|path| crate::files::read_regular(path, 65536).map_err(err))
            .transpose()?;
        Ok(TlsOptions { roots_pem })
    }
    pub(super) fn imap(&self) -> Result<ImapClient, HostError> {
        Ok(ImapClient::new(
            endpoint(&self.config.imap),
            self.tls()?,
            self.credentials()?,
        ))
    }
}
