// SPDX-License-Identifier: GPL-3.0-only
use crate::mime::Budget;
use crate::{ContentError, SafeLink, sanitize_text};
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
fn blocked(tag: &str) -> bool {
    matches!(
        tag,
        "script"
            | "style"
            | "svg"
            | "math"
            | "iframe"
            | "object"
            | "embed"
            | "form"
            | "input"
            | "button"
            | "textarea"
            | "select"
            | "template"
            | "head"
            | "link"
            | "meta"
            | "base"
            | "noscript"
            | "audio"
            | "video"
            | "source"
            | "track"
    )
}
fn block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "tr"
    )
}
fn safe_target(target: &str) -> bool {
    if target.chars().count() > 2048
        || target
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\\' | '<' | '>' | '"' | '\''))
        || sanitize_text(target) != target
    {
        return false;
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        let Some((_, rest)) = target.split_once("://") else {
            return false;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        !authority.is_empty()
            && !authority.contains('@')
            && !authority.contains('%')
            && !authority.starts_with(':')
            && !authority.ends_with(':')
    } else {
        lower.starts_with("mailto:") && target.len() > 7 && !target[7..].contains(['?', '#', ':'])
    }
}
fn newline(text: &mut String, budget: &mut Budget) {
    if !text.is_empty() && !text.ends_with('\n') {
        text.push_str(&budget.take("\n", usize::MAX));
    }
}
enum Visit {
    Enter(Handle),
    Exit {
        tag: String,
        link: Option<(String, usize)>,
    },
}
pub(crate) fn render_html(
    html: &str,
    max_depth: usize,
    budget: &mut Budget,
    text: &mut String,
    links: &mut Vec<SafeLink>,
) -> Result<(), ContentError> {
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    // Check all nodes, including discarded subtrees, before emitting any HTML text.
    let mut depth_stack = vec![(dom.document.clone(), 0usize)];
    while let Some((node, depth)) = depth_stack.pop() {
        if depth > max_depth {
            return Err(ContentError::HtmlDepthExceeded);
        }
        if let NodeData::Element {
            template_contents, ..
        } = &node.data
            && let Some(contents) = template_contents.borrow().as_ref()
        {
            depth_stack.push((contents.clone(), depth + 1));
        }
        for child in node.children.borrow().iter() {
            depth_stack.push((child.clone(), depth + 1));
        }
    }
    let mut stack = vec![Visit::Enter(dom.document.clone())];
    while let Some(visit) = stack.pop() {
        match visit {
            Visit::Enter(node) => {
                let mut exit = None;
                match &node.data {
                    NodeData::Text { contents } => {
                        let safe = sanitize_text(&contents.borrow());
                        let mut normalized = String::new();
                        let mut whitespace =
                            text.ends_with(|c: char| c.is_whitespace()) || text.is_empty();
                        for c in safe.chars() {
                            if c.is_whitespace() {
                                if !whitespace {
                                    normalized.push(' ');
                                }
                                whitespace = true;
                            } else {
                                normalized.push(c);
                                whitespace = false;
                            }
                        }
                        text.push_str(&budget.take(&normalized, usize::MAX));
                    }
                    NodeData::Element { name, attrs, .. } => {
                        let tag = name.local.as_ref();
                        if name.ns.as_ref() != "http://www.w3.org/1999/xhtml" || blocked(tag) {
                            continue;
                        }
                        if block(tag) {
                            newline(text, budget);
                        }
                        if tag == "br" || tag == "hr" {
                            newline(text, budget);
                        }
                        if tag == "li" {
                            text.push_str(&budget.take("- ", usize::MAX));
                        }
                        if matches!(tag, "td" | "th") && !text.is_empty() && !text.ends_with('\n') {
                            text.push_str(&budget.take(" | ", usize::MAX));
                        }
                        if tag == "img" {
                            let attributes = attrs.borrow();
                            let src = attributes
                                .iter()
                                .find(|a| a.name.local.as_ref() == "src")
                                .map(|a| a.value.to_ascii_lowercase());
                            let has_srcset =
                                attributes.iter().any(|a| a.name.local.as_ref() == "srcset");
                            let placeholder = match src.as_deref() {
                                Some(s) if s.starts_with("cid:") => "[CID image omitted]",
                                Some(s)
                                    if s.starts_with("http:")
                                        || s.starts_with("https:")
                                        || s.starts_with("//") =>
                                {
                                    "[remote image blocked]"
                                }
                                _ if has_srcset => "[remote image blocked]",
                                _ => "[image omitted]",
                            };
                            text.push_str(&budget.take(placeholder, usize::MAX));
                        }
                        let link = if tag == "a" {
                            attrs
                                .borrow()
                                .iter()
                                .find(|a| a.name.local.as_ref() == "href")
                                .map(|a| a.value.to_string())
                                .filter(|s| safe_target(s))
                                .map(|s| (s, text.len()))
                        } else {
                            None
                        };
                        exit = Some(Visit::Exit {
                            tag: tag.to_owned(),
                            link,
                        });
                    }
                    _ => {}
                }
                if let Some(exit) = exit {
                    stack.push(exit);
                }
                for child in node.children.borrow().iter().rev() {
                    stack.push(Visit::Enter(child.clone()));
                }
            }
            Visit::Exit { tag, link } => {
                if let Some((target, start)) = link {
                    let label: String = text[start..].trim().chars().take(256).collect();
                    let id = links.len() + 1;
                    let marker = format!(" [{id}]");
                    let need =
                        target.chars().count() + label.chars().count() + marker.chars().count();
                    if links.len() < 128 && budget.remaining >= need {
                        let target = budget.take(&target, usize::MAX);
                        let label = budget.take(&label, usize::MAX);
                        text.push_str(&budget.take(&marker, usize::MAX));
                        links.push(SafeLink { id, label, target });
                    } else {
                        budget.truncated = true;
                    }
                }
                if block(&tag) {
                    newline(text, budget);
                }
            }
        }
    }
    Ok(())
}
