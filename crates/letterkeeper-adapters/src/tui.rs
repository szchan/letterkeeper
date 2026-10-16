// SPDX-License-Identifier: GPL-3.0-only
use crate::{BrowserState, BrowserWorker, Host, MessageSummary, inert_text, render_dynamic};
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use letterkeeper_content::SafeDocument;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{io, sync::Arc};
#[derive(Default, Debug)]
pub struct UiState {
    pub selected: usize,
    pub reading: bool,
    pub quit: bool,
    pub help: bool,
    pub scroll: u16,
}
impl UiState {
    pub fn key(&mut self, k: KeyCode, len: usize) {
        match k {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Enter | KeyCode::Tab => {
                self.reading = !self.reading;
                self.scroll = 0
            }
            KeyCode::Esc => self.reading = false,
            KeyCode::Down | KeyCode::Char('j') => {
                if self.reading {
                    self.scroll = self.scroll.saturating_add(1)
                } else {
                    self.selected = (self.selected + 1).min(len.saturating_sub(1))
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.reading {
                    self.scroll = self.scroll.saturating_sub(1)
                } else {
                    self.selected = self.selected.saturating_sub(1)
                }
            }
            _ => (),
        }
    }
}
pub fn render(
    frame: &mut Frame,
    state: &UiState,
    messages: &[MessageSummary],
    documents: &[SafeDocument],
) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(frame.area());
    frame.render_widget(
        Paragraph::new("LetterKeeper · 合成演示 / 受控协议")
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL)),
        areas[0],
    );
    let text = if state.help {
        "j/k ↑/↓ 选择或滚动\nEnter/Tab 阅读/列表 · Esc 取消/返回 · q 退出\nr 刷新 · n/p 分页 · f 文件夹 · / 搜索\n邮件是不可信数据；链接和附件不会自动打开。".to_owned()
    } else if state.reading {
        documents
            .get(state.selected)
            .map(|d| {
                format!(
                    "{}\n发件人：{}\n\n{}\n\n链接编号：{}\n附件数量：{}",
                    inert_text(&d.subject, 256),
                    inert_text(&d.from, 256),
                    format_args!(
                        "{}{}",
                        if d.truncated || d.text.chars().count() > 16384 {
                            "［正文已截断，请通过 CLI 审阅完整内容］\n"
                        } else {
                            ""
                        },
                        inert_text(&d.text, 16384)
                    ),
                    d.links
                        .iter()
                        .take(50)
                        .map(|l| l.id.to_string())
                        .collect::<Vec<_>>()
                        .join("，"),
                    d.attachments.len()
                )
            })
            .unwrap_or_else(|| "没有邮件".into())
    } else {
        String::new()
    };
    if !state.reading && !state.help {
        let items = messages
            .iter()
            .map(|m| {
                ListItem::new(format!(
                    "{}\n  {}",
                    inert_text(&m.subject, 120),
                    inert_text(&m.from, 100)
                ))
            })
            .collect::<Vec<_>>();
        let list = List::new(items)
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("不可信邮件文本"),
            );
        let mut selection = ListState::default();
        selection.select(
            (!messages.is_empty()).then_some(state.selected.min(messages.len().saturating_sub(1))),
        );
        frame.render_stateful_widget(list, areas[1], &mut selection);
    } else {
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .scroll((if state.reading { state.scroll } else { 0 }, 0))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("不可信邮件文本"),
                ),
            areas[1],
        );
    }
    frame.render_widget(
        Paragraph::new("↑↓ j/k · Enter Tab 阅读 · ? 帮助 · q 退出"),
        areas[2],
    );
}
pub(crate) struct TerminalGuard;
impl TerminalGuard {
    pub(crate) fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), DisableBracketedPaste, LeaveAlternateScreen);
    }
}
pub fn run_tui(host: Arc<dyn Host>) -> io::Result<()> {
    let mut browser = BrowserWorker::new(host);
    let _guard = TerminalGuard::enter()?;
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(io::stdout()))?;
    let mut state = BrowserState::default();
    while !state.ui.quit {
        browser.poll(&mut state);
        terminal.draw(|frame| render_dynamic(frame, &state))?;
        if event::poll(std::time::Duration::from_millis(30))?
            && let Event::Key(key) = event::read()?
            && key.kind == event::KeyEventKind::Press
        {
            state.key(key.code);
        }
    }
    Ok(())
}
