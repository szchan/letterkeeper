// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_test_support::mail::{Scenario, Server};

#[test]
fn shared_tls_endpoint_preserves_synthetic_scenario() {
    let server = Server::start(Scenario::Imap);
    let client = letterkeeper_protocol::ImapClient::new(
        letterkeeper_protocol::Endpoint { host: server.endpoint.host.clone(), port: server.endpoint.port },
        letterkeeper_protocol::TlsOptions { roots_pem: server.tls.roots_pem.clone() },
        letterkeeper_protocol::Credentials { username: "person@example.test".into(), secret: "synthetic-secret".into(), oauth: false },
    );
    assert_eq!(client.list_folders().unwrap()[0].name, "INBOX");
    let transcript = server.finish();
    assert!(transcript.iter().any(|line| line.contains("LIST")));
    assert!(!transcript.iter().any(|line| line.contains("STORE")));
}
