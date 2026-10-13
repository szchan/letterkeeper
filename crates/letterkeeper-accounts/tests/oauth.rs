// SPDX-License-Identifier: GPL-3.0-only
mod support {
 include!("accounts.rs");
}
use letterkeeper_accounts::*;
use std::{io::{Read,Write},net::{TcpListener,TcpStream},thread,time::Duration};
fn oauth_config()->AccountConfig {let mut c=support::config();c.authentication=Authentication::GmailOAuth{client_id:"synthetic-client".into(),client_secret_ref:false};c}
fn server(responses:Vec<(u16,String)>)->(String,thread::JoinHandle<Vec<String>>) {
 let listener=TcpListener::bind("127.0.0.1:0").unwrap();let base=format!("http://{}",listener.local_addr().unwrap());
 let handle=thread::spawn(move||{let mut requests=Vec::new();for(status,body)in responses{
 let(mut s,_)=listener.accept().unwrap();s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut request=Vec::new();let mut buf=[0;1024];loop{let n=s.read(&mut buf).unwrap();request.extend_from_slice(&buf[..n]);if let Some(i)=request.windows(4).position(|x|x==b"\r\n\r\n"){let head=String::from_utf8_lossy(&request[..i]);let length=head.lines().find_map(|l|l.to_ascii_lowercase().strip_prefix("content-length: ").and_then(|x|x.parse::<usize>().ok())).unwrap_or(0);if request.len()>=i+4+length{break;}}}
 requests.push(String::from_utf8(request).unwrap());let response=format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());s.write_all(response.as_bytes()).unwrap();}requests});(base,handle)
}
fn token(access:&str,refresh:Option<&str>,scope:&str)->String{let mut v=serde_json::json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"scope":scope});if let Some(r)=refresh{v["refresh_token"]=r.into();}v.to_string()}
fn callback(login:&OAuthLogin,tail:&str)->thread::JoinHandle<()> {
 let url=url::Url::parse(login.authorization_url()).unwrap();let state=url.query_pairs().find(|(k,_)|k=="state").unwrap().1.into_owned();let redirect=url::Url::parse(login.redirect_uri()).unwrap();let addr=format!("127.0.0.1:{}",redirect.port().unwrap());let path=format!("/?state={state}&{tail}");thread::spawn(move||{let mut s=TcpStream::connect(&addr).unwrap();s.write_all(format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes()).unwrap();let mut out=String::new();s.read_to_string(&mut out).unwrap();})
}
#[test]
fn code_exchange_refresh_rotation_cache_logout_and_revoke() {
 let(base,handle)=server(vec![(200,token("access1",Some("refresh1"),GMAIL_SCOPE)),(200,token("access2",None,GMAIL_SCOPE)),(200,token("access3",Some("refresh3"),GMAIL_SCOPE)),(200,String::new())]);let store=support::FakeStore::default();let c=oauth_config();
 let oauth=GmailOAuth::for_loopback_fixture(&c,&store,&base).unwrap();let mut login=oauth.begin().unwrap();let url=url::Url::parse(login.authorization_url()).unwrap();assert!(url.query_pairs().any(|(k,v)|k=="code_challenge_method"&&v=="S256"));assert!(!format!("{login:?}").contains("state="));
 let browser=callback(&login,"code=synthetic-code");oauth.complete(&mut login,Duration::from_secs(3)).unwrap();browser.join().unwrap();assert!(oauth.complete(&mut login,Duration::from_millis(20)).is_err());
 let a=oauth.auth_material(0).unwrap();assert_eq!(a.secret.expose(),"access1");assert!(a.oauth);
 let a=oauth.auth_material(u64::MAX/2).unwrap();assert_eq!(a.secret.expose(),"access2");let stored=store.get(&c.id,SecretKind::OAuthTokens).unwrap().unwrap();assert!(stored.expose().contains("refresh1"));
 oauth.auth_material(u64::MAX/2+4000).unwrap();oauth.revoke().unwrap();assert!(store.get(&c.id,SecretKind::OAuthTokens).unwrap().is_none());oauth.logout().unwrap();let requests=handle.join().unwrap();assert!(requests[0].contains("code_verifier="));assert!(requests[1].contains("refresh_token=refresh1"));assert!(requests[3].contains("token=refresh3"));
}
#[test]
fn callback_denial_timeout_state_duplicates_and_scope_failure() {
 let c=oauth_config();let store=support::FakeStore::default();let oauth=GmailOAuth::new(&c,&store).unwrap();let mut login=oauth.begin().unwrap();let browser=callback(&login,"error=access_denied&error_description=SYNTHETIC_SECRET");let e=oauth.complete(&mut login,Duration::from_secs(2)).unwrap_err();browser.join().unwrap();assert_eq!(e,AccountError::AuthorizationDenied);assert!(!format!("{e:?}").contains("SYNTHETIC"));
 let mut login=oauth.begin().unwrap();assert_eq!(oauth.complete(&mut login,Duration::from_millis(20)).unwrap_err(),AccountError::Timeout);
 let mut login=oauth.begin().unwrap();let browser=callback(&login,"code=a&state=wrong");assert_eq!(oauth.complete(&mut login,Duration::from_secs(2)).unwrap_err(),AccountError::InvalidCallback);browser.join().unwrap();
 let(base,handle)=server(vec![(200,token("a",Some("r"),"profile"))]);let oauth=GmailOAuth::for_loopback_fixture(&c,&store,&base).unwrap();let mut login=oauth.begin().unwrap();let browser=callback(&login,"code=a");assert_eq!(oauth.complete(&mut login,Duration::from_secs(2)).unwrap_err(),AccountError::InsufficientScope);browser.join().unwrap();handle.join().unwrap();assert!(store.get(&c.id,SecretKind::OAuthTokens).unwrap().is_none());
 assert!(GmailOAuth::for_loopback_fixture(&c,&store,"http://example.test").is_err());
}
#[test]
fn invalid_grant_and_http_error_body_are_redacted() {
 let(base,handle)=server(vec![(200,token("a",Some("r"),GMAIL_SCOPE)),(400,r#"{"error":"invalid_grant","error_description":"SYNTHETIC_SECRET"}"#.into())]);let c=oauth_config();let store=support::FakeStore::default();let oauth=GmailOAuth::for_loopback_fixture(&c,&store,&base).unwrap();let mut login=oauth.begin().unwrap();let browser=callback(&login,"code=a");oauth.complete(&mut login,Duration::from_secs(2)).unwrap();browser.join().unwrap();assert_eq!(oauth.auth_material(u64::MAX/2).unwrap_err(),AccountError::ReauthenticationRequired);handle.join().unwrap();
}
