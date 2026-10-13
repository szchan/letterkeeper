// SPDX-License-Identifier: GPL-3.0-only
mod support;
use letterkeeper_accounts::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};
fn oauth_config() -> AccountConfig {
    let mut c = support::config();
    c.authentication = Authentication::GmailOAuth {
        client_id: "synthetic-client".into(),
        client_secret_ref: false,
    };
    c
}
fn server(responses: Vec<(u16, String)>) -> (String, thread::JoinHandle<Vec<String>>) {
    server_with_delay(responses, Duration::ZERO)
}
fn server_with_delay(
    responses: Vec<(u16, String)>,
    delay: Duration,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 1024];
            loop {
                let n = s.read(&mut buf).unwrap();
                request.extend_from_slice(&buf[..n]);
                if let Some(i) = request.windows(4).position(|x| x == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..i]);
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|x| x.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= i + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(request).unwrap());
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            thread::sleep(delay);
            s.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    (base, handle)
}
fn token(access: &str, refresh: Option<&str>, scope: &str) -> String {
    let mut v = serde_json::json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"scope":scope});
    if let Some(r) = refresh {
        v["refresh_token"] = r.into();
    }
    v.to_string()
}
fn callback(login: &OAuthLogin, tail: &str) -> thread::JoinHandle<()> {
    let url = url::Url::parse(login.authorization_url()).unwrap();
    let state = url
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let redirect = url::Url::parse(login.redirect_uri()).unwrap();
    let addr = format!("127.0.0.1:{}", redirect.port().unwrap());
    let path = format!("/?state={state}&{tail}");
    thread::spawn(move || {
        let mut s = TcpStream::connect(&addr).unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
    })
}
#[test]
fn code_exchange_refresh_rotation_cache_logout_and_revoke() {
    let (base, handle) = server(vec![
        (200, token("access1", Some("refresh1"), GMAIL_SCOPE)),
        (200, token("access2", None, GMAIL_SCOPE)),
        (200, token("access3", Some("refresh3"), GMAIL_SCOPE)),
        (200, String::new()),
    ]);
    let store = support::FakeStore::default();
    let c = oauth_config();
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let url = url::Url::parse(login.authorization_url()).unwrap();
    assert!(
        url.query_pairs()
            .any(|(k, v)| k == "code_challenge_method" && v == "S256")
    );
    assert!(!format!("{login:?}").contains("state="));
    let browser = callback(&login, "code=synthetic-code");
    oauth.complete(&mut login, Duration::from_secs(3)).unwrap();
    browser.join().unwrap();
    assert!(
        oauth
            .complete(&mut login, Duration::from_millis(20))
            .is_err()
    );
    let a = oauth.auth_material(0).unwrap();
    assert_eq!(a.secret.expose(), "access1");
    assert!(a.oauth);
    let a = oauth.auth_material(u64::MAX / 2).unwrap();
    assert_eq!(a.secret.expose(), "access2");
    let stored = store.get(&c.id, SecretKind::OAuthTokens).unwrap().unwrap();
    assert!(stored.expose().contains("refresh1"));
    oauth.auth_material(u64::MAX / 2 + 4000).unwrap();
    oauth.revoke().unwrap();
    assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_none());
    oauth.logout().unwrap();
    let requests = handle.join().unwrap();
    assert!(requests[0].contains("code_verifier="));
    assert!(requests[1].contains("refresh_token=refresh1"));
    assert!(requests[3].contains("token=refresh3"));
}
#[test]
fn callback_denial_timeout_state_duplicates_and_scope_failure() {
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::new(&c, &store).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(
        &login,
        "error=access_denied&error_description=SYNTHETIC_SECRET",
    );
    let e = oauth
        .complete(&mut login, Duration::from_secs(2))
        .unwrap_err();
    browser.join().unwrap();
    assert_eq!(e, AccountError::AuthorizationDenied);
    assert!(!format!("{e:?}").contains("SYNTHETIC"));
    let mut login = oauth.begin().unwrap();
    assert_eq!(
        oauth
            .complete(&mut login, Duration::from_millis(20))
            .unwrap_err(),
        AccountError::Timeout
    );
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a&state=wrong");
    assert_eq!(
        oauth
            .complete(&mut login, Duration::from_secs(2))
            .unwrap_err(),
        AccountError::InvalidCallback
    );
    browser.join().unwrap();
    let (base, handle) = server(vec![(200, token("a", Some("r"), "profile"))]);
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a");
    assert_eq!(
        oauth
            .complete(&mut login, Duration::from_secs(2))
            .unwrap_err(),
        AccountError::InsufficientScope
    );
    browser.join().unwrap();
    handle.join().unwrap();
    assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_none());
    assert!(GmailOAuth::for_loopback_fixture(&c, &store, "http://example.test").is_err());
}
#[test]
fn invalid_grant_and_http_error_body_are_redacted() {
    let (base, handle) = server(vec![
        (200, token("a", Some("r"), GMAIL_SCOPE)),
        (
            400,
            r#"{"error":"invalid_grant","error_description":"SYNTHETIC_SECRET"}"#.into(),
        ),
    ]);
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a");
    oauth.complete(&mut login, Duration::from_secs(2)).unwrap();
    browser.join().unwrap();
    assert_eq!(
        oauth.auth_material(u64::MAX / 2).unwrap_err(),
        AccountError::ReauthenticationRequired
    );
    handle.join().unwrap();
}

#[test]
fn rejected_callback_shapes_do_not_contact_the_token_service() {
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::new(&c, &store).unwrap();
    for tail in [
        "code=a&code=b",
        "code=a&error=access_denied",
        "code=",
        "code=a&scope=profile",
    ] {
        let mut login = oauth.begin().unwrap();
        let browser = callback(&login, tail);
        assert!(oauth.complete(&mut login, Duration::from_secs(1)).is_err());
        browser.join().unwrap();
    }
    for request in [
        "GET /?state=wrong&code=a HTTP/1.1\r\nHost: HOST\r\n\r\n",
        "POST /?code=a HTTP/1.1\r\nHost: HOST\r\n\r\n",
        "GET /?code=a HTTP/1.1\r\nHost: evil.example.test\r\n\r\n",
        "GET /other?code=a HTTP/1.1\r\nHost: HOST\r\n\r\n",
    ] {
        let mut login = oauth.begin().unwrap();
        let redirect = url::Url::parse(login.redirect_uri()).unwrap();
        let addr = format!("127.0.0.1:{}", redirect.port().unwrap());
        let request = request.replace("HOST", &addr);
        let browser = thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(request.as_bytes()).unwrap();
            let _ = s.read_to_end(&mut Vec::new());
        });
        assert_eq!(
            oauth
                .complete(&mut login, Duration::from_secs(1))
                .unwrap_err(),
            AccountError::InvalidCallback
        );
        browser.join().unwrap();
    }
    assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_none());
}
#[test]
fn callback_stall_is_bounded() {
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::new(&c, &store).unwrap();
    let mut login = oauth.begin().unwrap();
    let url = url::Url::parse(login.redirect_uri()).unwrap();
    let mut peer = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
    peer.write_all(b"GET ").unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        oauth
            .complete(&mut login, Duration::from_millis(30))
            .unwrap_err(),
        AccountError::Timeout
    );
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn token_bounds_type_lifetime_refresh_scope_and_failed_revoke() {
    for body in [
        token("a", None, GMAIL_SCOPE),
        token("a", Some("r"), "profile"),
        r#"{"access_token":"a","refresh_token":"r","expires_in":0,"token_type":"Bearer"}"#.into(),
        r#"{"access_token":"a","refresh_token":"r","expires_in":3600,"token_type":"mac"}"#.into(),
        "x".repeat(65537),
    ] {
        let (base, handle) = server(vec![(200, body)]);
        let c = oauth_config();
        let store = support::FakeStore::default();
        let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
        let mut login = oauth.begin().unwrap();
        let browser = callback(&login, "code=a");
        assert!(oauth.complete(&mut login, Duration::from_secs(1)).is_err());
        browser.join().unwrap();
        handle.join().unwrap();
        assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_none());
    }
    let (base, handle) = server(vec![
        (200, token("a", Some("r"), GMAIL_SCOPE)),
        (500, "SYNTHETIC_SECRET".into()),
    ]);
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a");
    oauth.complete(&mut login, Duration::from_secs(1)).unwrap();
    browser.join().unwrap();
    assert_eq!(
        oauth.revoke().unwrap_err(),
        AccountError::OAuthRequestFailed
    );
    assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_some());
    oauth.logout().unwrap();
    assert!(store.get(&c.id, SecretKind::OAuthTokens).unwrap().is_none());
    handle.join().unwrap();
}
#[test]
fn configured_client_secret_is_vault_only_and_login_state_is_random() {
    let mut c = oauth_config();
    c.authentication = Authentication::GmailOAuth {
        client_id: "synthetic-client".into(),
        client_secret_ref: true,
    };
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::new(&c, &store).unwrap();
    assert_eq!(oauth.begin().unwrap_err(), AccountError::MissingCredential);
    store
        .set(
            &c.id,
            SecretKind::ClientSecret,
            &Secret::new("SYNTHETIC_CLIENT_SECRET".into()),
        )
        .unwrap();
    let a = oauth.begin().unwrap();
    let b = oauth.begin().unwrap();
    assert_ne!(a.authorization_url(), b.authorization_url());
    assert!(!a.authorization_url().contains("SYNTHETIC_CLIENT_SECRET"));
    let json = serde_json::to_string(&c).unwrap();
    assert!(!json.contains("SYNTHETIC_CLIENT_SECRET"));
}

#[test]
fn oauth_login_is_bound_to_the_originating_account() {
    let first = oauth_config();
    let mut second = oauth_config();
    second.id = "other".into();
    let store = support::FakeStore::default();
    let owner = GmailOAuth::new(&first, &store).unwrap();
    let other = GmailOAuth::new(&second, &store).unwrap();
    let mut login = owner.begin().unwrap();
    assert_eq!(
        other
            .complete(&mut login, Duration::from_millis(20))
            .unwrap_err(),
        AccountError::InvalidConfiguration
    );
}
#[test]
fn refresh_is_serialized_across_instances_in_one_process() {
    let (base, handle) = server_with_delay(
        vec![
            (200, token("a", Some("r"), GMAIL_SCOPE)),
            (200, token("renewed", Some("rotated"), GMAIL_SCOPE)),
            (200, String::new()),
        ],
        Duration::from_millis(50),
    );
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a");
    oauth.complete(&mut login, Duration::from_secs(1)).unwrap();
    browser.join().unwrap();
    let oauth2 = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    thread::scope(|scope| {
        let a = scope.spawn(|| oauth.auth_material(u64::MAX / 2));
        let b = scope.spawn(|| oauth2.auth_material(u64::MAX / 2));
        assert_eq!(a.join().unwrap().unwrap().secret.expose(), "renewed");
        assert_eq!(b.join().unwrap().unwrap().secret.expose(), "renewed");
    });
    oauth.revoke().unwrap();
    assert_eq!(handle.join().unwrap().len(), 3);
}

#[test]
fn http_redirect_is_rejected_without_contacting_the_target() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let redirect = target.local_addr().unwrap();
    let service = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0; 4096];
        assert!(stream.read(&mut buf).unwrap() > 0);
        stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{redirect}/unexpected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).unwrap();
    });
    let c = oauth_config();
    let store = support::FakeStore::default();
    let oauth = GmailOAuth::for_loopback_fixture(&c, &store, &base).unwrap();
    let mut login = oauth.begin().unwrap();
    let browser = callback(&login, "code=a");
    assert_eq!(
        oauth
            .complete(&mut login, Duration::from_secs(1))
            .unwrap_err(),
        AccountError::OAuthRequestFailed
    );
    browser.join().unwrap();
    service.join().unwrap();
    assert!(target.accept().is_err());
}
