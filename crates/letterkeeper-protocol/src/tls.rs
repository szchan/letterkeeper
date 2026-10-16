// SPDX-License-Identifier: GPL-3.0-only
use crate::smtp::SmtpProgress;
use crate::validation::valid;
use crate::wire::{MAX_HEADER, State, Wire};
use crate::{Endpoint, ProtocolError, SmtpStage, TlsOptions};
use std::sync::Arc;
pub(crate) async fn connect(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    state: Arc<State>,
    imap: bool,
) -> Result<Wire, ProtocolError> {
    connect_with_progress(endpoint, tls, state, imap, None).await
}
/// Validate SMTP endpoint and trust configuration without DNS, connection or authentication.
/// Uses the same default roots, extra CA bounds and strict TLS policy as connect.
pub fn validate_smtp_tls(endpoint: &Endpoint, tls: &TlsOptions) -> Result<(), ProtocolError> {
    tls_configuration(endpoint, tls).map(|_| ())
}
fn tls_configuration(
    endpoint: &Endpoint,
    tls: &TlsOptions,
) -> Result<(rustls::pki_types::ServerName<'static>, rustls::ClientConfig), ProtocolError> {
    if !valid(&endpoint.host, 253) || endpoint.port == 0 {
        return Err(ProtocolError::InvalidInput);
    }
    let name = rustls::pki_types::ServerName::try_from(endpoint.host.clone())
        .map_err(|_| ProtocolError::InvalidInput)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(pem) = &tls.roots_pem {
        if pem.len() > MAX_HEADER {
            return Err(ProtocolError::InvalidInput);
        }
        let mut found = false;
        for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
            roots
                .add(cert.map_err(|_| ProtocolError::InvalidInput)?)
                .map_err(|_| ProtocolError::InvalidInput)?;
            found = true;
        }
        if !found {
            return Err(ProtocolError::InvalidInput);
        }
    }
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|_| ProtocolError::Connection)?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok((name, config))
}
pub(crate) async fn connect_with_progress(
    endpoint: &Endpoint,
    tls: &TlsOptions,
    state: Arc<State>,
    imap: bool,
    progress: Option<&SmtpProgress>,
) -> Result<Wire, ProtocolError> {
    if let Some(progress) = progress {
        progress.set(SmtpStage::TlsConfiguration);
    }
    let (name, config) = tls_configuration(endpoint, tls)?;
    if let Some(progress) = progress {
        progress.set(SmtpStage::Connection);
    }
    let tcp = tokio::net::TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .await
        .map_err(|_| ProtocolError::Connection)?;
    if let Some(progress) = progress {
        progress.set(SmtpStage::TlsHandshake);
    }
    let stream = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|_| ProtocolError::Connection)?;
    Ok(Wire::new(stream, state, imap))
}
