// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_accounts::*;
use std::{collections::HashMap, sync::Mutex, os::unix::fs::{PermissionsExt, symlink}};

pub fn config() -> AccountConfig {
    serde_json::from_str(r#"{"id":"synthetic","email":"sender@example.test","username":"sender@example.test","imap":{"host":"imap.example.test","port":993},"smtp":{"host":"smtp.example.test","port":465},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"append","authentication":{"type":"password"},"extra_ca_pem":null}"#).unwrap()
}
#[derive(Default)]
pub struct FakeStore(pub Mutex<HashMap<(String,SecretKind),String>>);
impl CredentialStore for FakeStore {
 fn get(&self,a:&str,k:SecretKind)->Result<Option<Secret>,AccountError>{ Ok(self.0.lock().unwrap().get(&(a.into(),k)).map(|s|Secret::new(s.clone()))) }
 fn set(&self,a:&str,k:SecretKind,s:&Secret)->Result<(),AccountError>{self.0.lock().unwrap().insert((a.into(),k),s.expose().into());Ok(())}
 fn delete(&self,a:&str,k:SecretKind)->Result<(),AccountError>{self.0.lock().unwrap().remove(&(a.into(),k));Ok(())}
}
#[test]
fn strict_configuration_namespace_scope_and_public_serialization() {
 let c=config();c.validate().unwrap();
 let mut value=serde_json::to_value(&c).unwrap();value["password"]="SYNTHETIC_SECRET";
 assert!(serde_json::from_value::<AccountConfig>(value).is_err());
 for id in ["../escape","","a/b","x\ny","Upper","a b"] {let mut c=config();c.id=id.into();assert!(c.validate().is_err());}
 let mut c=config();c.allowed_folders=vec!["INBOX".into()];assert!(c.validate().is_err());
 let mut c=config();c.smtp.port=0;assert!(c.validate().is_err());
 let mut c=config();c.imap.host="bad\r\n.example.test".into();assert!(c.validate().is_err());
 let mut c=config();c.email="bad@example.test\r\n".into();assert!(c.validate().is_err());
}
#[test]
fn secrets_redact_and_password_uses_account_namespace() {
 let s=Secret::new("SYNTHETIC_SECRET".into());assert!(!format!("{s:?}").contains("SYNTHETIC_SECRET"));
 let store=FakeStore::default();assert!(password_auth_material(&config(),&store).is_err());
 store.set("other",SecretKind::Password,&s).unwrap();assert!(password_auth_material(&config(),&store).is_err());
 store.set("synthetic",SecretKind::Password,&s).unwrap();let a=password_auth_material(&config(),&store).unwrap();assert!(!a.oauth);assert_eq!(a.secret.expose(),"SYNTHETIC_SECRET");
}
#[test]
fn private_repository_atomic_roundtrip_and_rejects_links_permissions_bounds() {
 let temp=tempfile::tempdir().unwrap();let repo=AccountRepository::new(temp.path().join("config"),temp.path().join("data")).unwrap();
 repo.save(&config()).unwrap();assert_eq!(repo.load("synthetic").unwrap(),config());assert_eq!(repo.list().unwrap(),vec!["synthetic"]);
 assert_eq!(std::fs::metadata(repo.config_dir()).unwrap().permissions().mode()&0o777,0o700);
 let p=repo.config_dir().join("synthetic.json");assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode()&0o777,0o600);
 std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o644)).unwrap();assert!(repo.load("synthetic").is_err());assert!(repo.save(&config()).is_err());
 std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o600)).unwrap();std::fs::hard_link(&p,repo.config_dir().join("other.json")).unwrap();assert!(repo.load("synthetic").is_err());assert!(repo.save(&config()).is_err());
 std::fs::remove_file(repo.config_dir().join("other.json")).unwrap();std::fs::remove_file(&p).unwrap();symlink(temp.path().join("outside"),&p).unwrap();assert!(repo.save(&config()).is_err());
 std::fs::remove_file(&p).unwrap();std::fs::write(&p,vec![b'x';MAX_CONFIG_BYTES+1]).unwrap();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(repo.load("synthetic").is_err());
 assert!(repo.load("../escape").is_err());
}
#[test]
fn repository_refuses_symlink_directory_and_public_existing_directory() {
 let temp=tempfile::tempdir().unwrap();std::fs::create_dir(temp.path().join("public")).unwrap();
 assert!(AccountRepository::new(temp.path().join("public"),temp.path().join("data")).is_err());
 symlink(temp.path().join("public"),temp.path().join("link")).unwrap();assert!(AccountRepository::new(temp.path().join("link"),temp.path().join("data")).is_err());
}
