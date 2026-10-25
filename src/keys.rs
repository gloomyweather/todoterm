//! Key handling per mode.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Mode, Target};

fn is_ctrl(key: &KeyEvent, c: char) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(c)
}

/// A printable character for text input. Ctrl/Alt chords are not text,
/// but Ctrl+Alt together is AltGr on many layouts and is let through.
fn typed_char(key: &KeyEvent) -> Option<char> {
    let KeyCode::Char(c) = key.code else { return None };
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    (ctrl == alt).then_some(c)
}

/// Map the Russian ЙЦУКЕН layout to the QWERTY keys in the same place,
/// so normal-mode shortcuts work without switching layouts.
fn qwerty(c: char) -> char {
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ.,";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>/?";
    RU.chars()
        .position(|r| r == c)
        .and_then(|i| EN.chars().nth(i))
        .unwrap_or(c)
}

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode {
        Mode::Normal => normal(app, key),
        Mode::Insert(_) => insert(app, key),
        Mode::Filter => filter(app, key),
        Mode::Move => move_mode(app, key),
        // Any key closes the help.
        Mode::Help => app.mode = Mode::Normal,
    }
}

pub fn handle_paste(app: &mut App, text: &str) {
    match app.mode {
        Mode::Insert(_) => app.editor.insert_str(text),
        Mode::Filter => {
            app.editor.insert_str(text);
            app.update_filter();
        }
        _ => {}
    }
}

fn normal(app: &mut App, key: KeyEvent) {
    if is_ctrl(&key, 'c') {
        app.should_quit = true;
        return;
    }
    if is_ctrl(&key, 'r') {
        app.redo();
        return;
    }
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let code = match key.code {
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            KeyCode::Char(qwerty(c))
        }
        KeyCode::Char(_) => return,
        other => other,
    };
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc if !app.filter.is_empty() => app.clear_filter(),
        KeyCode::Esc => app.should_quit = true,

        KeyCode::Down if shift => app.move_selected(true),
        KeyCode::Up if shift => app.move_selected(false),
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Char('g') | KeyCode::Home => app.first(),
        KeyCode::Char('G') | KeyCode::End => app.last(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::PageUp => app.page_up(),

        KeyCode::Char('a') | KeyCode::Char('i') => app.start_insert(Target::Append),
        KeyCode::Char('o') => app.start_insert(Target::Below),
        KeyCode::Char('O') => app.start_insert(Target::Above),
        KeyCode::Char('e') | KeyCode::Char('r') => app.start_edit(),

        KeyCode::Char(' ') | KeyCode::Enter => app.toggle(),
        KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => app.delete(),
        KeyCode::Char('J') => app.move_selected(true),
        KeyCode::Char('K') => app.move_selected(false),
        KeyCode::Char('m') => app.start_move(),
        KeyCode::Char('p') => app.cycle_priority(),

        KeyCode::Char('/') => app.start_filter(),
        KeyCode::Char('h') => app.toggle_hide_done(),
        KeyCode::Char('C') => app.clear_done(),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('?') | KeyCode::F(1) => app.mode = Mode::Help,
        _ => {}
    }
}

/// Keys shared by the task input and the filter input.
/// Returns true when the text may have changed.
fn edit_line(app: &mut App, key: &KeyEvent) -> bool {
    let e = &mut app.editor;
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('a') => e.home(),
            KeyCode::Char('e') => e.end(),
            KeyCode::Char('b') => e.left(),
            KeyCode::Char('f') => e.right(),
            KeyCode::Char('w') | KeyCode::Backspace => {
                e.delete_word_back();
                return true;
            }
            KeyCode::Char('u') => {
                e.delete_to_start();
                return true;
            }
            KeyCode::Char('h') => {
                e.backspace();
                return true;
            }
            _ => {}
        }
        if typed_char(key).is_none() {
            return false;
        }
    }
    match key.code {
        KeyCode::Left => e.left(),
        KeyCode::Right => e.right(),
        KeyCode::Home => e.home(),
        KeyCode::End => e.end(),
        KeyCode::Backspace => e.backspace(),
        KeyCode::Delete => e.delete(),
        _ => match typed_char(key) {
            Some(c) => e.insert_char(c),
            None => return false,
        },
    }
    true
}

fn insert(app: &mut App, key: KeyEvent) {
    if is_ctrl(&key, 'c') || key.code == KeyCode::Esc {
        app.cancel_input();
        return;
    }
    if key.code == KeyCode::Enter {
        app.submit_input();
        return;
    }
    edit_line(app, &key);
}

fn filter(app: &mut App, key: KeyEvent) {
    if is_ctrl(&key, 'c') || key.code == KeyCode::Esc {
        app.clear_filter();
        return;
    }
    match key.code {
        KeyCode::Enter => app.commit_filter(),
        KeyCode::Down => app.next(),
        KeyCode::Up => app.previous(),
        _ => {
            if edit_line(app, &key) {
                app.update_filter();
            }
        }
    }
}

fn move_mode(app: &mut App, key: KeyEvent) {
    if is_ctrl(&key, 'c') {
        app.mode = Mode::Normal;
        return;
    }
    let code = match key.code {
        KeyCode::Char(c) => KeyCode::Char(qwerty(c)),
        other => other,
    };
    match code {
        KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down => app.move_selected(true),
        KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up => app.move_selected(false),
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('m') | KeyCode::Char('q') => app.mode = Mode::Normal,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            handle_key(app, ch(c));
        }
    }

    #[test]
    fn ctrl_c_cancels_input_without_typing() {
        let mut app = App::new(Vec::new());
        handle_key(&mut app, ch('a'));
        type_str(&mut app, "hi");
        handle_key(&mut app, ctrl('c'));
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.tasks.is_empty());
        assert!(!app.should_quit);
        handle_key(&mut app, ctrl('c'));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_and_alt_chords_are_not_text() {
        let mut app = App::new(Vec::new());
        handle_key(&mut app, ch('a'));
        type_str(&mut app, "ab");
        handle_key(&mut app, KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT));
        handle_key(&mut app, ctrl('z'));
        assert_eq!(app.editor.text(), "ab");
        // AltGr arrives as Ctrl+Alt.
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL | KeyModifiers::ALT),
        );
        handle_key(&mut app, KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT));
        assert_eq!(app.editor.text(), "ab@B");
        handle_key(&mut app, ctrl('w'));
        assert_eq!(app.editor.text(), "");
    }

    #[test]
    fn add_edit_flow() {
        let mut app = App::new(Vec::new());
        handle_key(&mut app, ch('a'));
        type_str(&mut app, "milk !!");
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.tasks[0].description, "milk");
        assert_eq!(app.tasks[0].priority, 2);
        handle_key(&mut app, ch('e'));
        handle_key(&mut app, ctrl('a'));
        type_str(&mut app, "oat ");
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.tasks[0].description, "oat milk");
        handle_key(&mut app, ch('u'));
        assert_eq!(app.tasks[0].description, "milk");
        handle_key(&mut app, ctrl('r'));
        assert_eq!(app.tasks[0].description, "oat milk");
    }

    #[test]
    fn esc_clears_filter_before_quitting() {
        let mut app = App::new(Vec::new());
        handle_key(&mut app, ch('/'));
        type_str(&mut app, "x");
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.filter, "x");
        handle_key(&mut app, key(KeyCode::Esc));
        assert_eq!(app.filter, "");
        assert!(!app.should_quit);
        handle_key(&mut app, key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn russian_layout_in_normal_mode() {
        let mut app = App::new(Vec::new());
        // "ф" is the "a" key.
        handle_key(&mut app, ch('ф'));
        assert_eq!(app.mode, Mode::Insert(Target::Append));
        // ...but in insert mode it is just a letter.
        type_str(&mut app, "фыв");
        handle_key(&mut app, key(KeyCode::Enter));
        assert_eq!(app.tasks[0].description, "фыв");
        handle_key(&mut app, ch('й'));
        assert!(app.should_quit);
    }

    #[test]
    fn help_closes_on_any_key() {
        let mut app = App::new(Vec::new());
        handle_key(&mut app, ch('?'));
        assert_eq!(app.mode, Mode::Help);
        handle_key(&mut app, ch('q'));
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.should_quit);
    }

    #[test]
    fn paste_into_input() {
        let mut app = App::new(Vec::new());
        handle_paste(&mut app, "ignored");
        handle_key(&mut app, ch('a'));
        handle_paste(&mut app, "line one\nline two");
        assert_eq!(app.editor.text(), "line one line two");
    }
}
