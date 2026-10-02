//! The help in a window of its own (`geodb-mirror --help-window`, or `(help-window)` / Ctrl-H in the mirror):
//! where we are (the working directory and the folders `(load "X.LSP")` looks in), the scripts found there, and
//! every function with its usage and an example. A click on a green line copies it to the clipboard, ready to
//! paste into the console with Ctrl-V; Shift-drag and Cmd-C copy any text.

use crate::repl;
use scopekit::crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use scopekit::ratatui::layout::{Constraint, Layout};
use scopekit::ratatui::style::{Color, Modifier, Style};
use scopekit::ratatui::text::Line;
use scopekit::ratatui::widgets::Paragraph;
use scopekit::ratatui::Frame;
use scopekit::{App, Flow, ViewSlot};

/// One row of the help: its text, and what a click copies.
struct Row {
    text: String,
    copy: Option<String>,
    style: Style,
}

pub struct Help {
    rows: Vec<Row>,
    width: usize,
    scroll: usize,
    page: usize,
    notice: String,
    body_top: u16,
}

fn wrap(text: &str, width: usize, indent: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::from(indent);
    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > width && !line.trim().is_empty() {
            lines.push(std::mem::replace(&mut line, indent.to_string()));
        }
        if !line.trim().is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.trim().is_empty() {
        lines.push(line);
    }
    lines
}

/// The code of an example line: what is before a `;` comment.
fn code_of(example: &str) -> String {
    example
        .split(';')
        .next()
        .unwrap_or(example)
        .trim()
        .to_string()
}

impl Help {
    pub fn new() -> Self {
        Help {
            rows: Vec::new(),
            width: 0,
            scroll: 0,
            page: 20,
            notice: String::new(),
            body_top: 1,
        }
    }

    fn build(&mut self, width: usize) {
        let head = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let plain = Style::default();
        let dim = Style::default().fg(Color::Gray);
        let copyable = Style::default().fg(Color::Green);
        let mut rows: Vec<Row> = Vec::new();
        let mut add =
            |text: String, copy: Option<String>, style: Style| rows.push(Row { text, copy, style });

        add("WHERE WE ARE".into(), None, head);
        let cwd =
            std::env::current_dir().map_or_else(|e| e.to_string(), |d| d.display().to_string());
        add(format!("  working directory: {cwd}"), None, plain);
        add(
            "  (load \"X.LSP\") looks there first, then in:".into(),
            None,
            dim,
        );
        for d in repl::script_dirs() {
            add(format!("    {}", d.display()), None, dim);
        }
        add(String::new(), None, plain);
        add("SCRIPTS YOU CAN LOAD  (click a green line: it is copied, paste it in the console with Ctrl-V)".into(), None, head);
        let scripts = repl::scripts();
        if scripts.is_empty() {
            add("  none found in those folders".into(), None, dim);
        }
        for (path, note) in scripts {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let call = format!("(load \"{name}\")");
            add(format!("  {call:<28} {note}"), Some(call), copyable);
        }
        add(String::new(), None, plain);
        add("FUNCTIONS".into(), None, head);
        let mut group = "";
        for d in repl::DOCS.iter().filter(|d| !d.group.is_empty()) {
            if d.group != group {
                group = d.group;
                add(String::new(), None, plain);
                add(group.to_string(), None, head);
            }
            add(
                format!("  {}", d.usage),
                Some(code_of(d.usage)).filter(|c| !c.contains('[') && !c.contains('|')),
                plain,
            );
            for l in wrap(d.text, width.saturating_sub(2), "      ") {
                add(l, None, dim);
            }
            add(
                format!("      {}", d.example),
                Some(code_of(d.example)),
                copyable,
            );
        }
        add(String::new(), None, plain);
        add("LISP".into(), None, head);
        for note in repl::LISP_NOTE {
            for l in wrap(note, width.saturating_sub(2), "  ") {
                add(l, None, dim);
            }
        }
        // (the usage lines with placeholders are not copied: only real code is)
        self.rows = rows;
        self.width = width;
    }

    fn scroll_by(&mut self, n: isize) {
        let top = self.rows.len().saturating_sub(self.page);
        self.scroll = self.scroll.saturating_add_signed(n).min(top);
    }
}

impl App for Help {
    fn draw(&mut self, f: &mut Frame, _views: &mut ViewSlot) {
        let area = f.area();
        let [title, body, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(area);
        let width = usize::from(body.width);
        if width != self.width {
            self.build(width);
        }
        self.page = usize::from(body.height);
        self.body_top = body.y;
        self.scroll = self.scroll.min(self.rows.len().saturating_sub(self.page));
        f.render_widget(
            Paragraph::new("GeoDB mirror: help   (wheel, arrows, PgUp/PgDn scroll; a click on a green line copies it; q closes)")
                .style(Style::default().add_modifier(Modifier::REVERSED)),
            title,
        );
        let lines: Vec<Line> = self
            .rows
            .iter()
            .skip(self.scroll)
            .take(self.page)
            .map(|r| Line::styled(r.text.clone(), r.style))
            .collect();
        f.render_widget(Paragraph::new(lines), body);
        f.render_widget(
            Paragraph::new(self.notice.clone()).style(Style::default().fg(Color::Cyan)),
            status,
        );
    }

    fn event(&mut self, event: Event) -> Flow {
        match event {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => return Flow::Quit,
                KeyCode::Up => self.scroll_by(-1),
                KeyCode::Down => self.scroll_by(1),
                KeyCode::PageUp => self.scroll_by(-(self.page as isize) + 1),
                KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_by(self.page as isize - 1),
                KeyCode::Home => self.scroll = 0,
                KeyCode::End => self.scroll = self.rows.len(),
                _ => {}
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => self.scroll_by(-3),
                MouseEventKind::ScrollDown => self.scroll_by(3),
                MouseEventKind::Down(MouseButton::Left) if m.row >= self.body_top => {
                    let i = self.scroll + usize::from(m.row - self.body_top);
                    if let Some(code) = self.rows.get(i).and_then(|r| r.copy.clone()) {
                        self.notice = match scopekit::clipboard::copy_text(&code) {
                            Ok(()) => {
                                format!("copied: {code}   (paste it in the console with Ctrl-V)")
                            }
                            Err(e) => format!("copy: {e}"),
                        };
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Flow::Continue
    }
}
