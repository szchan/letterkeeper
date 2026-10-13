// SPDX-License-Identifier: GPL-3.0-only
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use letterkeeper_adapters::{WorkspaceState, review_text};
use letterkeeper_adapters::workspace::{WorkspaceCapabilities, ReviewSnapshot};
use letterkeeper_core::{Draft, DraftInput};
fn caps() -> WorkspaceCapabilities { WorkspaceCapabilities { account:"test".into(), from:"me@example.test".into(), read:true, draft:true, send:true } }
fn key(code:KeyCode)->Event { Event::Key(KeyEvent::new(code,KeyModifiers::NONE)) }
#[test]
fn editor_q_is_text_and_dirty_escape_requires_decision() {
 let mut s=WorkspaceState::new(caps()); s.event(key(KeyCode::Char('c'))); s.event(key(KeyCode::Char('q')));
 assert_eq!(s.editor.as_ref().unwrap().fields[0].lines(), &["q"]);
 s.event(key(KeyCode::Esc)); assert!(s.dialog.is_some()); assert!(!s.quit);
 s.event(key(KeyCode::Char('c'))); assert!(s.editor.is_some());
}
#[test]
fn paste_is_atomic_and_dangerous_controls_rejected() {
 let mut s=WorkspaceState::new(caps()); s.event(key(KeyCode::Char('c')));
 s.event(Event::Paste("中文q\nF9".into())); assert!(!s.quit);
 let before=s.editor.as_ref().unwrap().fields[0].lines().to_vec();
 s.event(Event::Paste("\x1b[31mSEND".into())); assert_eq!(s.editor.as_ref().unwrap().fields[0].lines(), before);
}
#[test]
fn full_review_preserves_tail_thread_and_escapes_reversibly() {
 let input=DraftInput { account:"a".into(), from:"m@example.test".into(), to:vec!["n@example.test".into()], cc:vec![],bcc:vec![],subject:"\\\x1b".into(),body:format!("{}TAIL", "中".repeat(18000)),attachments:vec![],in_reply_to:Some("<thread@test>".into()),references:vec!["<ref@test>".into()] };
 let text=review_text(&ReviewSnapshot {draft:Draft{id:1,revision:2,digest:"digest".into(),input},attachment_digests:vec![]});
 assert!(text.contains("TAIL")); assert!(text.contains("<thread@test>")); assert!(text.contains("<ref@test>")); assert!(text.contains("\\u{001b}")); assert!(!text.contains('\x1b'));
}
#[test]
fn body_budget_and_tab_focus_are_bounded() {
 let mut s=WorkspaceState::new(caps()); s.event(key(KeyCode::Char('c')));
 for _ in 0..4 { s.event(key(KeyCode::Tab)); }
 s.event(Event::Paste("x".repeat(1_048_576)));
 assert_eq!(s.editor.as_ref().unwrap().fields[4].lines()[0].len(),1_048_576);
 s.event(key(KeyCode::Char('x')));
 assert_eq!(s.editor.as_ref().unwrap().fields[4].lines()[0].len(),1_048_576);
}
#[test]
fn read_only_cannot_open_editor_or_approve() {
 let mut c=caps(); c.draft=false; c.send=false;
 let mut s=WorkspaceState::new(c); s.event(key(KeyCode::Char('c'))); assert!(s.editor.is_none());
 s.event(key(KeyCode::F(8))); assert!(s.dialog.is_none());
}
