// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    AccountConfig, AccountError, AuthMaterial, Authentication, CredentialStore, Secret, SecretKind,
};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, HttpRequest, HttpResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl,
    RefreshToken, RequestTokenError, Scope, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicErrorResponseType, BasicTokenResponse, BasicTokenType},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};
pub const GMAIL_SCOPE: &str = "https://mail.google.com/";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const MAX_RESPONSE: u64 = 64 * 1024;
const MAX_CALLBACK: usize = 8 * 1024;
// Vault bundle updates are serialized across OAuth clients within this process.
static TOKEN_MUTATION: std::sync::Mutex<()> = std::sync::Mutex::new(());
type Client = BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;
/// Login URL is for a trusted, user-facing CLI only; it is not MCP output.
pub struct OAuthLogin {
    owner: String,
    url: String,
    redirect: String,
    state: CsrfToken,
    verifier: Option<PkceCodeVerifier>,
    listener: Option<TcpListener>,
}
impl OAuthLogin {
    pub fn authorization_url(&self) -> &str {
        &self.url
    }
    pub fn redirect_uri(&self) -> &str {
        &self.redirect
    }
}
impl fmt::Debug for OAuthLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OAuthLogin([REDACTED])")
    }
}
pub struct GmailOAuth<'a> {
    account: &'a AccountConfig,
    store: &'a dyn CredentialStore,
    auth_url: String,
    token_url: String,
    revoke_url: String,
    http: reqwest::blocking::Client,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenBundle {
    access: String,
    refresh: String,
    expires_at: u64,
}
impl Drop for TokenBundle {
    fn drop(&mut self) {
        self.access.zeroize();
        self.refresh.zeroize();
    }
}
impl<'a> GmailOAuth<'a> {
    pub fn new(
        account: &'a AccountConfig,
        store: &'a dyn CredentialStore,
    ) -> Result<Self, AccountError> {
        Self::build(account, store, AUTH_URL, TOKEN_URL, REVOKE_URL)
    }
    /// Explicit fixture adapter. This does not accept internet hosts or alter production config.
    pub fn for_loopback_fixture(
        account: &'a AccountConfig,
        store: &'a dyn CredentialStore,
        base: &str,
    ) -> Result<Self, AccountError> {
        let url = url::Url::parse(base).map_err(|_| AccountError::InvalidConfiguration)?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(AccountError::InvalidConfiguration);
        }
        let base = base.trim_end_matches('/');
        Self::build(
            account,
            store,
            &format!("{base}/auth"),
            &format!("{base}/token"),
            &format!("{base}/revoke"),
        )
    }
    fn build(
        account: &'a AccountConfig,
        store: &'a dyn CredentialStore,
        auth: &str,
        token: &str,
        revoke: &str,
    ) -> Result<Self, AccountError> {
        account.validate()?;
        if !matches!(account.authentication, Authentication::GmailOAuth { .. }) {
            return Err(AccountError::InvalidConfiguration);
        }
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        Ok(Self {
            account,
            store,
            auth_url: auth.into(),
            token_url: token.into(),
            revoke_url: revoke.into(),
            http,
        })
    }
    fn client(&self, redirect: Option<&str>) -> Result<Client, AccountError> {
        let Authentication::GmailOAuth {
            client_id,
            client_secret_ref,
        } = &self.account.authentication
        else {
            return Err(AccountError::InvalidConfiguration);
        };
        let mut client = BasicClient::new(ClientId::new(client_id.clone()))
            .set_auth_uri(
                AuthUrl::new(self.auth_url.clone())
                    .map_err(|_| AccountError::InvalidConfiguration)?,
            )
            .set_token_uri(
                TokenUrl::new(self.token_url.clone())
                    .map_err(|_| AccountError::InvalidConfiguration)?,
            )
            .set_auth_type(AuthType::RequestBody);
        if *client_secret_ref {
            let secret = self
                .store
                .get(&self.account.id, SecretKind::ClientSecret)?
                .ok_or(AccountError::MissingCredential)?;
            if secret.expose().is_empty() || secret.expose().len() > 4096 {
                return Err(AccountError::CredentialUnavailable);
            }
            client = client.set_client_secret(ClientSecret::new(secret.expose().into()));
        }
        if let Some(redirect) = redirect {
            client = client.set_redirect_uri(
                RedirectUrl::new(redirect.into())
                    .map_err(|_| AccountError::InvalidConfiguration)?,
            )
        }
        Ok(client)
    }
    pub fn begin(&self) -> Result<OAuthLogin, AccountError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        let redirect = format!(
            "http://127.0.0.1:{}/",
            listener
                .local_addr()
                .map_err(|_| AccountError::OAuthRequestFailed)?
                .port()
        );
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let (url, state) = self
            .client(Some(&redirect))?
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(GMAIL_SCOPE.into()))
            .set_pkce_challenge(challenge)
            .add_extra_param("access_type", "offline")
            .add_extra_param("prompt", "consent")
            .url();
        Ok(OAuthLogin {
            owner: self.owner_key(),
            url: url.to_string(),
            redirect,
            state,
            verifier: Some(verifier),
            listener: Some(listener),
        })
    }
    fn owner_key(&self) -> String {
        let Authentication::GmailOAuth { client_id, .. } = &self.account.authentication else {
            unreachable!("validated OAuth configuration")
        };
        format!("{}\0{}\0{}", self.account.id, client_id, self.token_url)
    }
    pub fn complete(&self, login: &mut OAuthLogin, timeout: Duration) -> Result<(), AccountError> {
        let listener = login.listener.take().ok_or(AccountError::AlreadyConsumed)?;
        let verifier = login.verifier.take().ok_or(AccountError::AlreadyConsumed)?;
        if login.owner != self.owner_key()
            || timeout.is_zero()
            || timeout > Duration::from_secs(300)
        {
            return Err(AccountError::InvalidConfiguration);
        }
        let code = receive_callback(listener, &login.redirect, &login.state, timeout)?;
        let _guard = TOKEN_MUTATION
            .lock()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        let response = self
            .client(Some(&login.redirect))?
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(verifier)
            .request(&|request| self.request(request))
            .map_err(token_error)?;
        self.persist(response, None, unix_now()?)
    }
    fn request(&self, request: HttpRequest) -> Result<HttpResponse, AccountError> {
        let response = self
            .http
            .request(request.method().clone(), request.uri().to_string())
            .headers(request.headers().clone())
            .body(request.body().clone())
            .send()
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        let status = response.status();
        let headers = response.headers().clone();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE)
        {
            return Err(AccountError::OAuthRequestFailed);
        }
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut body)
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        if body.len() as u64 > MAX_RESPONSE {
            body.zeroize();
            return Err(AccountError::OAuthRequestFailed);
        }
        let mut result = HttpResponse::new(body);
        *result.status_mut() = status;
        *result.headers_mut() = headers;
        Ok(result)
    }
    fn persist(
        &self,
        response: BasicTokenResponse,
        old_refresh: Option<&str>,
        now: u64,
    ) -> Result<(), AccountError> {
        if response.token_type() != &BasicTokenType::Bearer {
            return Err(AccountError::OAuthRequestFailed);
        }
        if let Some(scopes) = response.scopes()
            && !scopes.iter().any(|s| s.as_str() == GMAIL_SCOPE)
        {
            return Err(AccountError::InsufficientScope);
        }
        let access = response.access_token().secret();
        let refresh = response
            .refresh_token()
            .map(|s| s.secret().as_str())
            .or(old_refresh)
            .ok_or(AccountError::ReauthenticationRequired)?;
        if !valid_token(access) || !valid_token(refresh) {
            return Err(AccountError::OAuthRequestFailed);
        }
        let expires = response
            .expires_in()
            .filter(|d| !d.is_zero() && d.as_secs() <= 86400 * 365)
            .ok_or(AccountError::OAuthRequestFailed)?;
        let bundle = TokenBundle {
            access: access.clone(),
            refresh: refresh.into(),
            expires_at: now
                .checked_add(expires.as_secs())
                .ok_or(AccountError::OAuthRequestFailed)?,
        };
        let encoded = Secret::new(
            serde_json::to_string(&bundle).map_err(|_| AccountError::CredentialUnavailable)?,
        );
        self.store
            .set(&self.account.id, SecretKind::OAuthTokens, &encoded)
    }
    fn tokens(&self) -> Result<TokenBundle, AccountError> {
        let encoded = self
            .store
            .get(&self.account.id, SecretKind::OAuthTokens)?
            .ok_or(AccountError::ReauthenticationRequired)?;
        if encoded.expose().len() > MAX_RESPONSE as usize {
            return Err(AccountError::CredentialUnavailable);
        }
        let tokens: TokenBundle = serde_json::from_str(encoded.expose())
            .map_err(|_| AccountError::CredentialUnavailable)?;
        if !valid_token(&tokens.access) || !valid_token(&tokens.refresh) {
            return Err(AccountError::CredentialUnavailable);
        }
        Ok(tokens)
    }
    /// Called before every protocol connection. A token within 60s of expiry is refreshed.
    pub fn auth_material(&self, now: u64) -> Result<AuthMaterial, AccountError> {
        let _guard = TOKEN_MUTATION
            .lock()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        let mut tokens = self.tokens()?;
        if tokens.expires_at <= now.saturating_add(60) {
            let response = self
                .client(None)?
                .exchange_refresh_token(&RefreshToken::new(tokens.refresh.clone()))
                .request(&|request| self.request(request))
                .map_err(token_error)?;
            self.persist(response, Some(&tokens.refresh), now)?;
            tokens = self.tokens()?;
        }
        Ok(AuthMaterial {
            username: self.account.username.clone(),
            secret: Secret::new(tokens.access.clone()),
            oauth: true,
        })
    }
    pub fn logout(&self) -> Result<(), AccountError> {
        let _guard = TOKEN_MUTATION
            .lock()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        self.store.delete(&self.account.id, SecretKind::OAuthTokens)
    }
    /// Explicit user action only. Local tokens are removed only after remote success.
    pub fn revoke(&self) -> Result<(), AccountError> {
        let _guard = TOKEN_MUTATION
            .lock()
            .map_err(|_| AccountError::CredentialUnavailable)?;
        let tokens = self.tokens()?;
        let response = self
            .http
            .post(&self.revoke_url)
            .form(&[("token", tokens.refresh.as_str())])
            .send()
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        let status = response.status();
        let mut body = Zeroizing::new(Vec::new());
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut body)
            .map_err(|_| AccountError::OAuthRequestFailed)?;
        if !status.is_success() || body.len() as u64 > MAX_RESPONSE {
            return Err(AccountError::OAuthRequestFailed);
        }
        self.store.delete(&self.account.id, SecretKind::OAuthTokens)
    }
}
pub fn unix_now() -> Result<u64, AccountError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| AccountError::OAuthRequestFailed)
}
fn valid_token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 16 * 1024 && s.bytes().all(|b| b.is_ascii_graphic())
}
fn token_error<E: std::error::Error>(
    error: RequestTokenError<E, oauth2::basic::BasicErrorResponse>,
) -> AccountError {
    match error {
        RequestTokenError::ServerResponse(response)
            if response.error() == &BasicErrorResponseType::InvalidGrant =>
        {
            AccountError::ReauthenticationRequired
        }
        _ => AccountError::OAuthRequestFailed,
    }
}
fn receive_callback(
    listener: TcpListener,
    redirect: &str,
    state: &CsrfToken,
    timeout: Duration,
) -> Result<String, AccountError> {
    listener
        .set_nonblocking(true)
        .map_err(|_| AccountError::OAuthRequestFailed)?;
    let deadline = Instant::now() + timeout;
    let (mut stream, peer) = loop {
        match listener.accept() {
            Ok(value) => break value,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(AccountError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return Err(AccountError::InvalidCallback),
        }
    };
    if !peer.ip().is_loopback() {
        return Err(AccountError::InvalidCallback);
    }
    let result = read_callback(&mut stream, redirect, state, deadline);
    let status = if result.is_ok() {
        "200 OK"
    } else {
        "400 Bad Request"
    };
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let body = "LetterKeeper login handled. Close this tab.\n";
    let length = body.len();
    let _=stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}").as_bytes());
    result
}
fn read_callback(
    stream: &mut TcpStream,
    redirect: &str,
    state: &CsrfToken,
    deadline: Instant,
) -> Result<String, AccountError> {
    let mut bytes = Zeroizing::new(Vec::new());
    let mut buffer = [0; 1024];
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .ok_or(AccountError::Timeout)?;
        stream
            .set_read_timeout(Some(left))
            .map_err(|_| AccountError::InvalidCallback)?;
        let n = stream.read(&mut buffer).map_err(|e| {
            if matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) {
                AccountError::Timeout
            } else {
                AccountError::InvalidCallback
            }
        })?;
        if n == 0 {
            return Err(AccountError::InvalidCallback);
        }
        bytes.extend_from_slice(&buffer[..n]);
        buffer.zeroize();
        if bytes.len() > MAX_CALLBACK {
            return Err(AccountError::InvalidCallback);
        }
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&bytes).map_err(|_| AccountError::InvalidCallback)?;
    let mut lines = request.split("\r\n");
    let first = lines.next().ok_or(AccountError::InvalidCallback)?;
    let words: Vec<_> = first.split(' ').collect();
    if words.len() != 3
        || words[0] != "GET"
        || words[2] != "HTTP/1.1"
        || !words[1].starts_with("/?")
        || words[1].contains('#')
    {
        return Err(AccountError::InvalidCallback);
    }
    let redirect_url = url::Url::parse(redirect).map_err(|_| AccountError::InvalidCallback)?;
    let expected_host = format!(
        "127.0.0.1:{}",
        redirect_url.port().ok_or(AccountError::InvalidCallback)?
    );
    let hosts: Vec<_> = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    if hosts.len() != 1 || hosts[0] != expected_host {
        return Err(AccountError::InvalidCallback);
    }
    let query = url::Url::parse(&format!("http://{expected_host}{}", words[1]))
        .map_err(|_| AccountError::InvalidCallback)?;
    let mut fields = HashMap::new();
    for (k, v) in query.query_pairs() {
        if fields.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err(AccountError::InvalidCallback);
        }
    }
    let returned = fields.get("state").ok_or(AccountError::InvalidCallback)?;
    if &CsrfToken::new(returned.clone()) != state {
        return Err(AccountError::InvalidCallback);
    }
    if fields.contains_key("error") {
        if fields.contains_key("code") {
            return Err(AccountError::InvalidCallback);
        }
        return Err(AccountError::AuthorizationDenied);
    }
    if let Some(scope) = fields.get("scope")
        && !scope.split_whitespace().any(|s| s == GMAIL_SCOPE)
    {
        return Err(AccountError::InsufficientScope);
    }
    let code = fields.remove("code").ok_or(AccountError::InvalidCallback)?;
    if !valid_token(&code) {
        return Err(AccountError::InvalidCallback);
    }
    Ok(code)
}
