// SPDX-License-Identifier: GPL-3.0-only
use super::*;
pub const MAX_EDITOR_BODY: usize = 1_048_576;
pub struct DraftEditor {
    pub fields: [TextArea<'static>; 5],
    pub focus: usize,
    pub saved: Option<Draft>,
    pub fixed: CoreInput,
    pub dirty: bool,
    pub generation: u64,
}
impl DraftEditor {
    pub fn new(input: CoreInput, saved: Option<Draft>) -> Self {
        let values = [
            input.to.join(", "),
            input.cc.join(", "),
            input.bcc.join(", "),
            input.subject.clone(),
            input.body.clone(),
        ];
        let fields = values.map(|v| {
            let mut field = TextArea::from(v.split('\n').map(str::to_owned).collect::<Vec<_>>());
            field.set_max_histories(20);
            field.set_style(Style::default().fg(Color::White).bg(Color::Black));
            field
        });
        Self {
            fields,
            focus: 0,
            saved,
            fixed: input,
            dirty: false,
            generation: 0,
        }
    }
    pub fn input(&self) -> CoreInput {
        let mut input = self.fixed.clone();
        let addresses = |index: usize| {
            self.fields[index]
                .lines()
                .join(" ")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        input.to = addresses(0);
        input.cc = addresses(1);
        input.bcc = addresses(2);
        input.subject = self.fields[3].lines().join(" ");
        input.body = self.fields[4].lines().join("\n");
        input
    }
    pub(super) fn edit(&mut self, event: Event) -> Result<(), &'static str> {
        let before = self.fields[self.focus].clone();
        let changed = match event {
            Event::Paste(text) => {
                if text
                    .chars()
                    .any(|c| dangerous(c) || (c.is_control() && c != '\n' && c != '\t'))
                {
                    return Err("拒绝危险粘贴控制字符");
                }
                if self.focus != 4 && text.contains(['\n', '\t']) {
                    return Err("地址和主题不接受多行粘贴");
                }
                if text.len() > MAX_EDITOR_BODY {
                    return Err("正文超过 1 MiB");
                }
                self.fields[self.focus].insert_str(text)
            }
            Event::Key(key) => {
                if let KeyCode::Char(c) = key.code
                    && (dangerous(c) || c.is_control())
                {
                    return Err("拒绝危险字符");
                }
                if self.focus != 4 && key.code == KeyCode::Enter {
                    return Ok(());
                }
                self.fields[self.focus].input(key)
            }
            _ => false,
        };
        let len = self.fields[self.focus]
            .lines()
            .iter()
            .map(String::len)
            .sum::<usize>()
            + self.fields[self.focus].lines().len().saturating_sub(1);
        let max = if self.focus == 4 {
            MAX_EDITOR_BODY
        } else if self.focus == 3 {
            256
        } else {
            20 * 256
        };
        if len > max {
            self.fields[self.focus] = before;
            return Err("编辑字段超过限制");
        }
        if changed {
            self.dirty = true;
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }
}
