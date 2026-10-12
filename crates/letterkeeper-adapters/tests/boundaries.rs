// SPDX-License-Identifier: GPL-3.0-only
use letterkeeper_adapters::{inert_text, UiState};
use crossterm::event::KeyCode;
#[test]
fn malicious_text_is_inert_and_bounded() {
 let s=inert_text("<img src='https://track.test'> ![x](http://x.test)\x1b[31m javascript:evil 中文",32);
 assert!(!s.contains('<')); assert!(!s.contains("https://")); assert!(!s.contains('[')); assert!(!s.contains('\x1b')); assert!(s.chars().count()<=32);
}
#[test]
fn navigation_is_bounded() {
 let mut ui=UiState::default(); ui.key(KeyCode::Down,0); assert_eq!(ui.selected,0);
 ui.key(KeyCode::Char('j'),3); assert_eq!(ui.selected,1);
 ui.key(KeyCode::Enter,3); assert!(ui.reading);
 ui.key(KeyCode::Tab,3); assert!(!ui.reading);
 ui.key(KeyCode::Char('q'),3); assert!(ui.quit);
}
