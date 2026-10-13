// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_accounts::*;
use std::{collections::HashMap, sync::Mutex};
pub fn config() -> AccountConfig {
    serde_json::from_str(r#"{"id":"synthetic","email":"sender@example.test","username":"sender@example.test","imap":{"host":"imap.example.test","port":993},"smtp":{"host":"smtp.example.test","port":465},"allowed_folders":["INBOX","Drafts","Sent"],"permissions":{"read":true,"draft":true,"send":true},"drafts_folder":"Drafts","sent_folder":"Sent","sent_copy":"append","authentication":{"type":"password"},"extra_ca_pem":null}"#).unwrap()
}
#[derive(Default)]
pub struct FakeStore(pub Mutex<HashMap<(String, SecretKind), String>>);
impl CredentialStore for FakeStore {
    fn get(&self, a: &str, k: SecretKind) -> Result<Option<Secret>, AccountError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&(a.into(), k))
            .map(|s| Secret::new(s.clone())))
    }
    fn set(&self, a: &str, k: SecretKind, s: &Secret) -> Result<(), AccountError> {
        self.0
            .lock()
            .unwrap()
            .insert((a.into(), k), s.expose().into());
        Ok(())
    }
    fn delete(&self, a: &str, k: SecretKind) -> Result<(), AccountError> {
        self.0.lock().unwrap().remove(&(a.into(), k));
        Ok(())
    }
}
