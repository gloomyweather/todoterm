//! Single-line text editor with a cursor, measured in chars.

use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
}

fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

impl LineEditor {
    pub fn with_text(text: &str) -> Self {
        let mut e = Self::default();
        e.insert_str(text);
        e
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert_char(&mut self, c: char) {
        let c = if c.is_control() { ' ' } else { c };
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Insert pasted text; line breaks and tabs become spaces.
    pub fn insert_str(&mut self, s: &str) {
        let s = s.replace("\r\n", " ");
        for c in s.chars() {
            self.insert_char(c);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Ctrl+W: delete the word before the cursor, plus the spaces after it.
    pub fn delete_word_back(&mut self) {
        let mut start = self.cursor;
        while start > 0 && self.chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !self.chars[start - 1].is_whitespace() {
            start -= 1;
        }
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Ctrl+U: delete everything before the cursor.
    pub fn delete_to_start(&mut self) {
        self.chars.drain(..self.cursor);
        self.cursor = 0;
    }

    /// The slice of text that fits in `width` columns with the cursor visible,
    /// and the cursor's column within it.
    pub fn view(&self, width: usize) -> (String, usize) {
        if width == 0 {
            return (String::new(), 0);
        }
        // Scroll so that the text before the cursor plus the cursor cell fit.
        let mut start = self.cursor;
        let mut before = 0;
        while start > 0 {
            let w = char_width(self.chars[start - 1]);
            if before + w + 1 > width {
                break;
            }
            before += w;
            start -= 1;
        }
        let mut out = String::new();
        let mut used = 0;
        for &c in &self.chars[start..] {
            let w = char_width(c);
            if used + w > width {
                break;
            }
            used += w;
            out.push(c);
        }
        (out, before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_moving() {
        let mut e = LineEditor::default();
        for c in "helo".chars() {
            e.insert_char(c);
        }
        e.left();
        e.insert_char('l');
        assert_eq!(e.text(), "hello");
        assert_eq!(e.cursor(), 4);
        e.home();
        e.delete();
        assert_eq!(e.text(), "ello");
        e.end();
        e.backspace();
        assert_eq!(e.text(), "ell");
        e.right();
        assert_eq!(e.cursor(), 3);
        e.home();
        e.left();
        e.backspace();
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn unicode() {
        let mut e = LineEditor::with_text("купить 牛奶");
        assert_eq!(e.cursor(), 9);
        e.backspace();
        assert_eq!(e.text(), "купить 牛");
        e.home();
        e.right();
        e.insert_char('ы');
        assert_eq!(e.text(), "кыупить 牛");
    }

    #[test]
    fn ctrl_w_and_u() {
        let mut e = LineEditor::with_text("buy some milk  ");
        e.delete_word_back();
        assert_eq!(e.text(), "buy some ");
        e.delete_word_back();
        assert_eq!(e.text(), "buy ");
        e.insert_str("tea");
        e.left();
        e.left();
        e.delete_to_start();
        assert_eq!(e.text(), "ea");
        assert_eq!(e.cursor(), 0);
        e.delete_word_back();
        assert_eq!(e.text(), "ea");
    }

    #[test]
    fn paste_flattens_newlines() {
        let e = LineEditor::with_text("a\nb\r\nc\td");
        assert_eq!(e.text(), "a b c d");
    }

    #[test]
    fn view_scrolls() {
        let e = LineEditor::with_text("abcdefghij");
        // Cursor at end needs one free cell.
        assert_eq!(e.view(5), ("ghij".into(), 4));
        let mut e = e;
        e.home();
        assert_eq!(e.view(5), ("abcde".into(), 0));
        assert_eq!(e.view(0), (String::new(), 0));
    }

    #[test]
    fn view_wide_chars() {
        let e = LineEditor::with_text("日本語テキスト");
        let (text, col) = e.view(7);
        assert_eq!(text, "キスト");
        assert_eq!(col, 6);
        let mut e = e;
        e.home();
        let (text, col) = e.view(7);
        assert_eq!(text, "日本語");
        assert_eq!(col, 0);
    }
}
