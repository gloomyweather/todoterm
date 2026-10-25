//! Drawing.

use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, Target};
use crate::task::{format_due, urgency, Urgency};
use crate::theme::Palette;

const SELECT_SYMBOL: &str = " › ";
const MOVE_SYMBOL: &str = " » ";

pub fn draw(frame: &mut Frame, app: &mut App, p: &Palette) {
    let area = frame.area();
    if area.width < 4 || area.height < 3 {
        return;
    }
    let roomy = area.height >= 8 && area.width >= 20;
    let inner = if roomy {
        area.inner(Margin::new(1, 1))
    } else {
        area
    };
    let bar = if roomy { 2 } else { 1 };
    let [header, list, footer] = Layout::vertical([
        Constraint::Length(bar),
        Constraint::Min(1),
        Constraint::Length(bar),
    ])
    .areas(inner);

    let border = |b: Borders| {
        if roomy {
            Block::new()
                .borders(b)
                .border_type(BorderType::Plain)
                .border_style(Style::new().fg(p.border))
        } else {
            Block::new()
        }
    };

    draw_header(frame, app, p, header, border(Borders::BOTTOM));
    draw_list(frame, app, p, list);
    draw_footer(frame, app, p, footer, border(Borders::TOP));
    if app.mode == Mode::Help {
        draw_help(frame, p, area);
    }
}

fn title(app: &App) -> (&'static str, &'static str) {
    match app.mode {
        Mode::Insert(Target::Edit(_)) => ("~ editing task ~", "(✐ ˘ ³˘)"),
        Mode::Insert(_) => ("~ adding task ~", "(✎ ˘ ³˘)"),
        Mode::Move => ("~ moving task ~", "(⬍ ˘ ³˘)"),
        _ => ("~ todo list ~", "(=^･ω･^=)"),
    }
}

fn draw_header(frame: &mut Frame, app: &App, p: &Palette, area: Rect, block: Block) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;

    let (text, face) = title(app);
    let mut left = vec![Span::styled(
        format!(" {text}"),
        Style::new().fg(p.fg).add_modifier(Modifier::BOLD),
    )];
    if app.kaomoji {
        left.push(Span::styled(format!("  {face}"), Style::new().fg(p.accent)));
    }

    // Right side, most important last so it survives when space runs out.
    let mut right: Vec<Span> = Vec::new();
    if matches!(app.mode, Mode::Insert(_)) {
        right.push(Span::styled("! priority · @date due", Style::new().fg(p.muted)));
    } else {
        if !app.filter.is_empty() {
            right.push(Span::styled(
                format!("filter: {}", app.filter),
                Style::new().fg(p.accent),
            ));
        }
        if app.hide_done {
            right.push(Span::styled("hiding done", Style::new().fg(p.muted)));
        }
        right.push(Span::styled(
            format!("{}/{}", app.done_count(), app.tasks.len()),
            Style::new().fg(p.fg),
        ));
    }
    if app.unsaved {
        right.push(Span::styled(
            "unsaved!",
            Style::new().fg(p.danger).add_modifier(Modifier::BOLD),
        ));
    }

    let left_w: usize = left.iter().map(|s| s.width()).sum();
    let right_w = |items: &[Span]| -> usize {
        items.iter().map(|s| s.width()).sum::<usize>() + 3 * items.len().saturating_sub(1)
    };
    // Drop right-side items from the front until everything fits with a space around.
    while !right.is_empty() && left_w + right_w(&right) + 2 > width {
        right.remove(0);
    }
    if right.is_empty() {
        frame.render_widget(Paragraph::new(truncate_spans(left, width)), inner);
        return;
    }
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(width - left_w - right_w(&right) - 1)));
    for (i, s) in right.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(p.muted)));
        }
        spans.push(s);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn truncate_spans(spans: Vec<Span<'_>>, width: usize) -> Line<'_> {
    let mut out = Vec::new();
    let mut used = 0;
    for s in spans {
        let w = s.width();
        if used + w <= width {
            used += w;
            out.push(s);
        } else {
            let content = truncate(&s.content, width - used);
            out.push(Span::styled(content, s.style));
            break;
        }
    }
    Line::from(out)
}

/// Cut `s` to `width` columns, ending with `…` when something was cut.
pub fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width - 1 {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push('…');
    out
}

fn draw_list(frame: &mut Frame, app: &mut App, p: &Palette, area: Rect) {
    app.page_size = area.height as usize;
    let visible = app.visible();

    if visible.is_empty() {
        let msg = if app.tasks.is_empty() {
            "no tasks left. you can rest now ૮ ˶ᵔ ᵕ ᵔ˶ ა".to_string()
        } else if !app.filter.is_empty() {
            format!("nothing matches “{}”", app.filter)
        } else {
            "all done, finished tasks are hidden ✧".to_string()
        };
        let lines = vec![
            Line::raw(""),
            Line::raw(truncate(&format!("   {msg}"), area.width as usize)),
        ];
        frame.render_widget(Paragraph::new(lines).style(Style::new().fg(p.muted)), area);
        return;
    }

    let today = (app.today)();
    let width = (area.width as usize).saturating_sub(SELECT_SYMBOL.width());
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(row, &i)| {
            let t = &app.tasks[i];
            let selected = row == app.cursor;
            let muted = Style::new().fg(p.muted);
            let text_style = if t.done {
                muted.add_modifier(Modifier::CROSSED_OUT)
            } else if selected && !p.reverse_selection {
                Style::new().fg(p.selection_fg)
            } else {
                Style::new().fg(p.fg)
            };

            let mut spans = vec![Span::styled(if t.done { "[x] " } else { "[ ] " }, muted)];
            let mut used = 4;
            if t.priority > 0 {
                let style = if t.done {
                    muted
                } else {
                    Style::new().fg(p.accent).add_modifier(Modifier::BOLD)
                };
                spans.push(Span::styled("!".repeat(t.priority as usize), style));
                spans.push(Span::raw(" "));
                used += t.priority as usize + 1;
            }
            let due = t.due.map(|d| {
                let style = match (t.done, urgency(d, today)) {
                    (true, _) | (_, Urgency::Later) => muted,
                    (_, Urgency::Overdue) => Style::new().fg(p.danger),
                    (_, Urgency::Today) => Style::new().fg(p.warning),
                };
                (format_due(d, today), style)
            });
            // Keep at least a few columns of text before showing the date.
            let due = due.filter(|(label, _)| used + label.width() + 6 <= width);
            let due_w = due.as_ref().map_or(0, |(label, _)| label.width() + 1);
            let text = truncate(&t.description, width.saturating_sub(used + due_w));
            used += text.width();
            spans.push(Span::styled(text, text_style));
            if let Some((label, style)) = due {
                spans.push(Span::raw(" ".repeat(width.saturating_sub(used + label.width()))));
                spans.push(Span::styled(label, style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let mut highlight = if p.reverse_selection {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new().bg(p.selection_bg)
    };
    let symbol = if app.mode == Mode::Move {
        highlight = highlight.add_modifier(Modifier::BOLD);
        MOVE_SYMBOL
    } else {
        SELECT_SYMBOL
    };
    let list = List::new(items)
        .highlight_style(highlight)
        .highlight_symbol(symbol)
        .highlight_spacing(HighlightSpacing::Always);
    let mut state = ListState::default()
        .with_offset(app.scroll)
        .with_selected(Some(app.cursor));
    frame.render_stateful_widget(list, area, &mut state);
    app.scroll = state.offset();
}

fn hints(p: &Palette, items: &[(&str, &str)], width: usize) -> Line<'static> {
    // "? help" is always last and always shown.
    let help = (" ?", " help");
    // Each item renders as " key desc ", the help item as " ? help".
    let item_w = |(k, d): &(&str, &str)| k.width() + d.width() + 3;
    let mut used = help.0.width() + help.1.width();
    let mut spans = Vec::new();
    for item in items {
        if used + item_w(item) > width {
            break;
        }
        used += item_w(item);
        spans.push(Span::styled(
            format!(" {}", item.0),
            Style::new().fg(p.fg).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {} ", item.1), Style::new().fg(p.muted)));
    }
    spans.push(Span::styled(
        help.0,
        Style::new().fg(p.fg).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(help.1, Style::new().fg(p.muted)));
    Line::from(spans)
}

fn draw_footer(frame: &mut Frame, app: &App, p: &Palette, area: Rect, block: Block) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;

    let prompt = match app.mode {
        Mode::Insert(_) => Some(" › "),
        Mode::Filter => Some(" / "),
        _ => None,
    };
    if let Some(prompt) = prompt {
        let (text, col) = app.editor.view(width.saturating_sub(prompt.width()));
        let line = Line::from(vec![
            Span::styled(prompt, Style::new().fg(p.accent).add_modifier(Modifier::BOLD)),
            Span::styled(text, Style::new().fg(p.fg)),
        ]);
        frame.render_widget(Paragraph::new(line), inner);
        frame.set_cursor_position((inner.x + (prompt.width() + col) as u16, inner.y));
        return;
    }

    let line = if let Some(status) = &app.status {
        let style = if status.error {
            Style::new().fg(p.danger).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(p.accent)
        };
        Line::from(Span::styled(truncate(&format!(" {}", status.text), width), style))
    } else {
        match app.mode {
            Mode::Move => hints(p, &[("j/k", "move"), ("enter", "done")], width),
            Mode::Help => Line::from(Span::styled(" any key to close", Style::new().fg(p.muted))),
            _ => hints(
                p,
                &[
                    ("a", "add"),
                    ("spc", "done"),
                    ("e", "edit"),
                    ("d", "delete"),
                    ("u", "undo"),
                    ("/", "filter"),
                    ("J/K", "move"),
                    ("p", "priority"),
                    ("h", "hide done"),
                    ("q", "quit"),
                ],
                width,
            ),
        }
    };
    frame.render_widget(Paragraph::new(line), inner);
}

const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "move around",
        &[
            ("j k  ↓ ↑", "next / previous"),
            ("g G  home end", "first / last"),
            ("pgup pgdn", "page up / down"),
        ],
    ),
    (
        "tasks",
        &[
            ("a i", "add at the end"),
            ("o O", "add below / above"),
            ("e r", "edit"),
            ("space enter", "mark done / undone"),
            ("d x", "delete"),
            ("J K", "move task down / up"),
            ("m", "move mode (j/k, enter)"),
            ("p", "cycle priority"),
            ("C", "clear done tasks"),
            ("u  ctrl+r", "undo / redo"),
        ],
    ),
    (
        "view",
        &[
            ("/", "filter, enter keeps, esc clears"),
            ("h", "hide done tasks"),
            ("q esc ctrl+c", "quit"),
        ],
    ),
    (
        "typing",
        &[
            ("ctrl+a ctrl+e", "start / end of line"),
            ("ctrl+w ctrl+u", "delete word / to start"),
            ("enter esc", "save / cancel"),
            ("! !! !!!", "priority"),
            ("@tom @fri @+3d", "due date, also @today @11-01"),
        ],
    ),
];

fn draw_help(frame: &mut Frame, p: &Palette, area: Rect) {
    let key_w = 16;
    let mut lines = Vec::new();
    for (i, (section, keys)) in HELP.iter().enumerate() {
        if i > 0 {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            format!(" {section}"),
            Style::new().fg(p.muted),
        )));
        for (k, d) in *keys {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("   {k:<key_w$}"),
                    Style::new().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*d, Style::new().fg(p.fg)),
            ]));
        }
    }
    let w = area.width.min(54);
    let h = area.height.min(lines.len() as u16 + 2);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(p.border))
        .title(Span::styled(
            " keys ",
            Style::new().fg(p.fg).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{parse_input, Task};
    use chrono::NaiveDate;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn app(texts: &[&str]) -> App {
        let tasks = texts
            .iter()
            .enumerate()
            .map(|(i, t)| Task::new(i as u32 + 1, parse_input(t, today())))
            .collect();
        let mut app = App::new(tasks);
        app.today = today;
        app
    }

    fn render(app: &mut App, w: u16, h: u16) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app, &Palette::terminal())).unwrap();
        term.backend().buffer().clone()
    }

    fn text(buf: &Buffer) -> String {
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn empty_list() {
        let s = text(&render(&mut app(&[]), 60, 12));
        assert!(s.contains("~ todo list ~"), "{s}");
        assert!(s.contains("no tasks left"), "{s}");
        assert!(s.contains("0/0"), "{s}");
        assert!(s.contains("? help"), "{s}");
    }

    #[test]
    fn task_row_layout() {
        let mut a = app(&["buy milk !! @tom", "done thing"]);
        a.tasks[1].set_done(true);
        let s = text(&render(&mut a, 50, 10));
        let row = s.lines().find(|l| l.contains("buy milk")).unwrap();
        assert!(row.contains(" › [ ] !! buy milk"), "{row}");
        // Due date right-aligned against the margin.
        assert!(row.trim_end().ends_with("tomorrow"), "{row}");
        assert_eq!(row.chars().count(), 50);
        assert!(row.ends_with("tomorrow "), "{row}");
        assert!(s.contains("[x] done thing"), "{s}");
        assert!(s.contains("1/2"), "{s}");
    }

    #[test]
    fn long_text_is_truncated() {
        let long = "word ".repeat(40);
        let mut a = app(&[&format!("{long} @today")]);
        let s = text(&render(&mut a, 40, 10));
        let row = s.lines().find(|l| l.contains("[ ]")).unwrap();
        assert!(row.contains('…'), "{row}");
        assert!(row.contains("today"), "{row}");
    }

    #[test]
    fn narrow_window() {
        let mut a = app(&["a fairly long task name here !!! @2027-01-01", "b"]);
        a.filter = "a".into();
        a.hide_done = true;
        let s = text(&render(&mut a, 40, 10));
        assert!(s.contains("todo list"), "{s}");
        assert!(s.contains("? help"), "{s}");
        // Tiny windows must not panic.
        for (w, h) in [(1, 1), (5, 3), (10, 4), (20, 5), (12, 8)] {
            render(&mut a, w, h);
        }
    }

    #[test]
    fn help_overlay() {
        let mut a = app(&["x"]);
        a.mode = Mode::Help;
        let s = text(&render(&mut a, 70, 40));
        assert!(s.contains("keys"), "{s}");
        assert!(s.contains("cycle priority"), "{s}");
        assert!(s.contains("any key to close"), "{s}");
        render(&mut a, 40, 10);
    }

    #[test]
    fn input_line_and_cursor() {
        let mut a = app(&[]);
        a.start_insert(Target::Append);
        a.editor.insert_str("hello");
        let mut term = Terminal::new(TestBackend::new(40, 10)).unwrap();
        term.draw(|f| draw(f, &mut a, &Palette::terminal())).unwrap();
        let s = text(term.backend().buffer());
        assert!(s.contains(" › hello"), "{s}");
        assert!(s.contains("adding task"), "{s}");
        term.backend_mut().assert_cursor_position((1 + 3 + 5, 8));
    }

    #[test]
    fn status_replaces_hints() {
        let mut a = app(&["x"]);
        a.delete();
        let s = text(&render(&mut a, 60, 10));
        assert!(s.contains("deleted · u to undo"), "{s}");
        assert!(!s.contains("? help"), "{s}");
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("日本語", 4), "日…");
        assert_eq!(truncate("abc", 0), "");
    }

    #[test]
    fn scrolls_to_selection() {
        let names: Vec<String> = (0..30).map(|i| format!("task {i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut a = app(&refs);
        a.last();
        let s = text(&render(&mut a, 40, 12));
        assert!(s.contains("task 29"), "{s}");
        assert!(!s.contains("task 0 "), "{s}");
    }
}
