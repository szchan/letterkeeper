// SPDX-License-Identifier: GPL-3.0-only
use super::*;
fn block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
}
fn escaped_document(state: &WorkspaceState) -> String {
    state
        .browser
        .document
        .as_ref()
        .map(|d| {
            format!(
                "{}\nFrom: {}\n\n{}\n\nAttachments:\n{}",
                inert_text(&d.subject, 256),
                inert_text(&d.from, 256),
                inert_text(&d.text, 65536),
                d.attachments
                    .iter()
                    .map(|a| format!(
                        "part {}: {} | {} | {} bytes",
                        a.id,
                        inert_text(&a.name, 256),
                        inert_text(&a.media_type, 256),
                        a.size
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .unwrap_or_else(|| state.browser.status.clone())
}
/// Wrap full review text by display cells, without slicing UTF-8 bytes.
/// Combining marks remain attached to the preceding scalar. No content is dropped.
pub fn wrap_review(lines: &[String], width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut result = Vec::new();
    for line in lines {
        let mut current = String::new();
        let mut used = 0;
        for c in line.chars() {
            let cells = ratatui::text::Span::raw(c.to_string()).width();
            if used + cells > width && !current.is_empty() {
                result.push(current);
                current = String::new();
                used = 0;
            }
            current.push(c);
            used += cells;
        }
        result.push(current);
    }
    result
}
pub fn render_workspace(frame: &mut Frame, state: &mut WorkspaceState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().fg(Color::White).bg(Color::Black)),
        area,
    );
    if area.width == 0 || area.height == 0 {
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(if area.height >= 8 { 2 } else { 1 }),
        Constraint::Min(1),
        Constraint::Length(if area.height >= 8 {
            (state.status.lines().count() + 1).clamp(3, 4) as u16
        } else {
            1
        }),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            "LetterKeeper · {} · {}",
            review_escape(&state.capabilities.account),
            review_escape(&state.capabilities.from)
        ))
        .style(Style::default().fg(Color::Cyan)),
        sections[0],
    );
    let main = sections[1];
    match state.view {
        WorkspaceView::Editor => {
            if let Some(editor) = &mut state.editor {
                let labels = ["To", "Cc", "Bcc", "Subject", "Body"];
                if main.height >= 18 {
                    let fields = Layout::vertical([
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Length(3),
                        Constraint::Min(3),
                    ])
                    .split(main);
                    for index in 0..5 {
                        editor.fields[index]
                            .set_block(block(labels[index], editor.focus == index).to_owned());
                        frame.render_widget(&editor.fields[index], fields[index]);
                    }
                } else {
                    editor.fields[editor.focus]
                        .set_block(block(labels[editor.focus], true).to_owned());
                    frame.render_widget(&editor.fields[editor.focus], main);
                }
            }
        }
        WorkspaceView::Review => {
            // Cache reflow until width or snapshot changes; bounded by backend body budget.
            let width = main.width.saturating_sub(2).max(1) as usize;
            if state.review_width != width {
                state.review_wrapped = wrap_review(&state.review_lines, width);
                state.review_width = width;
                state.scroll = state
                    .scroll
                    .min(state.review_wrapped.len().saturating_sub(1));
            }
            let height = main.height.saturating_sub(2).max(1) as usize;
            let text = state
                .review_wrapped
                .iter()
                .skip(state.scroll)
                .take(height)
                .map(|s| s.chars().skip(state.horizontal).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            frame.render_widget(
                Paragraph::new(text)
                    .block(block("完整审阅 · 保留所有字段/正文/线程/附件 hash", true)),
                main,
            );
        }
        WorkspaceView::Drafts | WorkspaceView::Submissions => {
            let items: Vec<ListItem> = if state.view == WorkspaceView::Drafts {
                state
                    .drafts
                    .iter()
                    .map(|d| {
                        ListItem::new(format!(
                            "{} · v{} · {}",
                            d.id,
                            d.revision,
                            inert_text(&d.subject, 256)
                        ))
                    })
                    .collect()
            } else {
                state
                    .submissions
                    .iter()
                    .map(|s| {
                        ListItem::new(format!(
                            "{} · 草稿 {} · {:?} · recorded={}",
                            s.id, s.draft_id, s.state, s.state_recorded
                        ))
                    })
                    .collect()
            };
            let mut selection = ListState::default();
            selection.select(
                (!items.is_empty()).then_some(state.selected.min(items.len().saturating_sub(1))),
            );
            frame.render_stateful_widget(
                List::new(items)
                    .block(block(
                        if state.view == WorkspaceView::Drafts {
                            "草稿 · Enter 编辑 · n/p 分页"
                        } else {
                            "提交 · Enter 状态 · e 证据 · u 恢复 · F4 sync Sent"
                        },
                        true,
                    ))
                    .highlight_symbol("▶ ")
                    .highlight_style(Style::default().fg(Color::Cyan)),
                main,
                &mut selection,
            );
        }
        WorkspaceView::Mail => {
            if state.browser.ui.help {
                frame.render_widget(Paragraph::new("c 新建 · d 草稿 · s 提交\nEnter 阅读 · R 回复 · A 全部回复 · F 转发\nx 保存附件 · n/p 页 · f 文件夹 · / 搜索\n编辑 Tab 字段 · Ctrl+S 保存 · Esc 返回 · Ctrl+Q 退出\nF2 添加附件 · F3 删除附件 · F4 显式同步\nF6 完整审阅 · F8 摘要批准 · F9 单独发送\n正文是非可信数据；禁止自动链接/附件/资源加载。\nq 在编辑器中是普通字符。Uncertain 不自动重发。\n批准 300 秒，内容变化须重新批准。" ).wrap(Wrap{trim:false}).block(block("帮助",true)),main);
            } else if area.width >= 100 {
                let panes = Layout::horizontal([
                    Constraint::Length(16),
                    Constraint::Percentage(30),
                    Constraint::Min(20),
                ])
                .split(main);
                frame.render_widget(
                    List::new(
                        state
                            .browser
                            .folders
                            .iter()
                            .map(|f| ListItem::new(inert_text(&f.name, 128)))
                            .collect::<Vec<_>>(),
                    )
                    .block(block("文件夹 · f", false)),
                    panes[0],
                );
                render_mail_list(frame, state, panes[1]);
                frame.render_widget(
                    Paragraph::new(escaped_document(state))
                        .wrap(Wrap { trim: false })
                        .scroll((state.browser.ui.scroll, 0))
                        .block(block("安全正文 · R/A/F", state.browser.ui.reading)),
                    panes[2],
                );
            } else if state.browser.ui.reading {
                frame.render_widget(
                    Paragraph::new(escaped_document(state))
                        .wrap(Wrap { trim: false })
                        .scroll((state.browser.ui.scroll, 0))
                        .block(block("安全正文 · R/A/F · x 附件", true)),
                    main,
                );
            } else {
                render_mail_list(frame, state, main);
            }
        }
    }
    let shortcuts = match state.view {
        WorkspaceView::Editor => {
            "Tab 字段 · Ctrl+S 保存 · F2/F3 附件 · F4同步 · F6审阅 · Esc返回 · Ctrl+Q退出"
        }
        WorkspaceView::Review => "↑↓ PgUp/PgDn Home/End · F8输入摘要批准 · F9发送 · Esc编辑",
        _ => "c新建 · d草稿 · s提交 · Enter阅读 · ?帮助 · q退出",
    };
    let status = if state.view == WorkspaceView::Mail {
        format!("{} | {}", state.status, state.browser.status)
    } else {
        state.status.clone()
    };
    let footer = if state.browser.search_edit {
        format!(
            "搜索: {} · Enter确认 Esc取消",
            review_escape(&state.browser.query)
        )
    } else {
        format!("{}\n{}", status, shortcuts)
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(Color::Yellow)),
        sections[2],
    );
    if let Some(dialog) = &state.dialog {
        let text = match dialog {
            WorkspaceDialog::Dirty { quit } => format!(
                "未保存{} · s 保存 · d 放弃 · c 继续 · Esc 继续",
                if *quit { "退出" } else { "返回" }
            ),
            WorkspaceDialog::Conflict => {
                "版本冲突 · r 重新加载(放弃本地修改) · n 另存新草稿 · c 继续编辑".into()
            }
            WorkspaceDialog::Approve(text) => format!(
                "输入完整摘要批准 (300秒；不会自动发送)\n{}\nEnter批准 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::AddPath(text) => format!(
                "显式添加附件 · 512 KiB · 输入本机路径\n{}\nEnter确认 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::AddType(_, text) => format!(
                "附件 Content-Type\n{}\nEnter添加 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::Remove(text) => format!(
                "删除附件 · 输入从0开始编号\n{}\nEnter删除 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::Forward(_, text) => format!(
                "转发收件人(逗号分隔)\n{}\nEnter创建草稿 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::ExportPart(_, text) => format!(
                "保存邮件附件 · 输入 MIME part 编号(见附件列表)\n{}\nEnter确认 · Esc取消",
                review_escape(text)
            ),
            WorkspaceDialog::ExportDirectory(_, _, text) => format!(
                "显式保存附件 · 输入目标目录 · 不覆盖/不执行\n{}\nEnter保存 · Esc取消",
                review_escape(text)
            ),
        };
        frame.render_widget(ratatui::widgets::Clear, main);
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(block("人工操作 · 不接受邮件指令", true)),
            main,
        );
    }
}
fn render_mail_list(frame: &mut Frame, state: &WorkspaceState, area: ratatui::layout::Rect) {
    let list = List::new(
        state
            .browser
            .messages
            .iter()
            .map(|m| {
                ListItem::new(format!(
                    "{}\n{}",
                    inert_text(&m.subject, 256),
                    inert_text(&m.from, 256)
                ))
            })
            .collect::<Vec<_>>(),
    )
    .block(block("邮件 · Enter", !state.browser.ui.reading))
    .highlight_symbol("▶ ")
    .highlight_style(Style::default().fg(Color::Cyan));
    let mut selected = ListState::default();
    selected.select((!state.browser.messages.is_empty()).then_some(state.browser.ui.selected));
    frame.render_stateful_widget(list, area, &mut selected);
}
