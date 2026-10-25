//! Application state and actions. Knows nothing about the terminal or disk:
//! changes set `dirty`, and the event loop saves.

use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate};

use crate::editor::LineEditor;
use crate::task::{parse_input, Task, MAX_PRIORITY};

const UNDO_LIMIT: usize = 100;
pub const STATUS_TTL: Duration = Duration::from_secs(3);

/// Where submitted input goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Append,
    Below,
    Above,
    Edit(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert(Target),
    Filter,
    Move,
    Help,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub error: bool,
    pub since: Instant,
}

pub struct App {
    pub tasks: Vec<Task>,
    pub mode: Mode,
    pub editor: LineEditor,
    pub filter: String,
    pub hide_done: bool,
    pub kaomoji: bool,
    /// Position of the selection within the visible tasks.
    pub cursor: usize,
    /// First visible row of the list, kept by the UI between frames.
    pub scroll: usize,
    /// Rows the list can show, set by the UI; used for PgUp/PgDn.
    pub page_size: usize,
    pub status: Option<Status>,
    /// Tasks changed and need saving.
    pub dirty: bool,
    /// The last save failed.
    pub unsaved: bool,
    pub should_quit: bool,
    undo: Vec<Vec<Task>>,
    redo: Vec<Vec<Task>>,
    move_checkpointed: bool,
    pub today: fn() -> NaiveDate,
}

fn local_today() -> NaiveDate {
    Local::now().date_naive()
}

impl App {
    pub fn new(tasks: Vec<Task>) -> Self {
        Self {
            tasks,
            mode: Mode::Normal,
            editor: LineEditor::default(),
            filter: String::new(),
            hide_done: false,
            kaomoji: true,
            cursor: 0,
            scroll: 0,
            page_size: 10,
            status: None,
            dirty: false,
            unsaved: false,
            should_quit: false,
            undo: Vec::new(),
            redo: Vec::new(),
            move_checkpointed: false,
            today: local_today,
        }
    }

    // ----- selection -----

    /// Indices into `tasks` of the tasks currently shown, in order.
    pub fn visible(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| !(self.hide_done && t.done) && t.matches(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// Index into `tasks` of the selected task.
    pub fn selected(&self) -> Option<usize> {
        self.visible().get(self.cursor).copied()
    }

    fn selected_id(&self) -> Option<u32> {
        self.selected().map(|i| self.tasks[i].id)
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.visible().len().saturating_sub(1));
    }

    /// Select the task with this id if visible, otherwise keep the cursor in range.
    fn select_id(&mut self, id: Option<u32>) {
        let visible = self.visible();
        if let Some(pos) = id.and_then(|id| visible.iter().position(|&i| self.tasks[i].id == id)) {
            self.cursor = pos;
        }
        self.clamp_cursor();
    }

    pub fn next(&mut self) {
        self.cursor += 1;
        self.clamp_cursor();
    }

    pub fn previous(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn first(&mut self) {
        self.cursor = 0;
    }

    pub fn last(&mut self) {
        self.cursor = usize::MAX;
        self.clamp_cursor();
    }

    pub fn page_down(&mut self) {
        self.cursor = self.cursor.saturating_add(self.page_size.max(1));
        self.clamp_cursor();
    }

    pub fn page_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(self.page_size.max(1));
    }

    // ----- history -----

    /// Remember the current tasks before a change.
    fn checkpoint(&mut self) {
        self.undo.push(self.tasks.clone());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn changed(&mut self) {
        self.dirty = true;
    }

    pub fn undo(&mut self) {
        match self.undo.pop() {
            Some(prev) => {
                let id = self.selected_id();
                self.redo.push(std::mem::replace(&mut self.tasks, prev));
                self.select_id(id);
                self.changed();
                self.info("undone");
            }
            None => self.info("nothing to undo"),
        }
    }

    pub fn redo(&mut self) {
        match self.redo.pop() {
            Some(next) => {
                let id = self.selected_id();
                self.undo.push(std::mem::replace(&mut self.tasks, next));
                self.select_id(id);
                self.changed();
                self.info("redone");
            }
            None => self.info("nothing to redo"),
        }
    }

    /// Replace the list after the file changed on disk.
    pub fn reload(&mut self, tasks: Vec<Task>) {
        let id = self.selected_id();
        self.tasks = tasks;
        // Old snapshots would silently undo someone else's change.
        self.undo.clear();
        self.redo.clear();
        if let Mode::Insert(Target::Edit(edit_id)) = self.mode {
            if !self.tasks.iter().any(|t| t.id == edit_id) {
                self.mode = Mode::Normal;
                self.editor = LineEditor::default();
            }
        }
        self.select_id(id);
        self.info("reloaded, file changed on disk");
    }

    // ----- status line -----

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: false,
            since: Instant::now(),
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: true,
            since: Instant::now(),
        });
    }

    /// Expire old status messages.
    pub fn tick(&mut self) {
        if self
            .status
            .as_ref()
            .is_some_and(|s| s.since.elapsed() >= STATUS_TTL)
        {
            self.status = None;
        }
    }

    // ----- input -----

    pub fn start_insert(&mut self, target: Target) {
        self.editor = LineEditor::default();
        self.mode = Mode::Insert(target);
    }

    pub fn start_edit(&mut self) {
        if let Some(i) = self.selected() {
            self.editor = LineEditor::with_text(&self.tasks[i].to_input());
            self.mode = Mode::Insert(Target::Edit(self.tasks[i].id));
        }
    }

    pub fn cancel_input(&mut self) {
        self.editor = LineEditor::default();
        self.mode = Mode::Normal;
    }

    pub fn submit_input(&mut self) {
        let Mode::Insert(target) = self.mode else { return };
        let parsed = parse_input(&self.editor.text(), (self.today)());
        self.cancel_input();
        if parsed.text.is_empty() {
            return;
        }

        if let Target::Edit(id) = target {
            let Some(i) = self.tasks.iter().position(|t| t.id == id) else {
                self.error("task no longer exists");
                return;
            };
            let t = &self.tasks[i];
            if t.description == parsed.text && t.priority == parsed.priority && t.due == parsed.due {
                return;
            }
            self.checkpoint();
            let t = &mut self.tasks[i];
            t.description = parsed.text;
            t.priority = parsed.priority;
            t.due = parsed.due;
            self.changed();
            return;
        }

        let id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let at = match (target, self.selected()) {
            (Target::Below, Some(i)) => i + 1,
            (Target::Above, Some(i)) => i,
            _ => self.tasks.len(),
        };
        self.checkpoint();
        self.tasks.insert(at, Task::new(id, parsed));
        self.changed();
        self.select_id(Some(id));
        if self.selected_id() != Some(id) {
            self.info("added, hidden by the filter");
        }
    }

    pub fn start_filter(&mut self) {
        self.editor = LineEditor::with_text(&self.filter);
        self.mode = Mode::Filter;
    }

    /// Apply the filter being typed.
    pub fn update_filter(&mut self) {
        let id = self.selected_id();
        self.filter = self.editor.text();
        self.select_id(id);
    }

    pub fn commit_filter(&mut self) {
        self.filter = self.editor.text().trim().to_string();
        self.editor = LineEditor::default();
        self.mode = Mode::Normal;
        self.clamp_cursor();
    }

    pub fn clear_filter(&mut self) {
        let id = self.selected_id();
        self.filter.clear();
        self.editor = LineEditor::default();
        self.mode = Mode::Normal;
        self.select_id(id);
    }

    // ----- task actions -----

    pub fn toggle(&mut self) {
        let Some(i) = self.selected() else { return };
        self.checkpoint();
        let done = !self.tasks[i].done;
        self.tasks[i].set_done(done);
        self.changed();
        self.clamp_cursor();
    }

    pub fn delete(&mut self) {
        let Some(i) = self.selected() else { return };
        self.checkpoint();
        self.tasks.remove(i);
        self.changed();
        self.clamp_cursor();
        self.info("deleted · u to undo");
    }

    pub fn cycle_priority(&mut self) {
        let Some(i) = self.selected() else { return };
        self.checkpoint();
        let t = &mut self.tasks[i];
        t.priority = (t.priority + 1) % (MAX_PRIORITY + 1);
        self.changed();
    }

    pub fn clear_done(&mut self) {
        let n = self.tasks.iter().filter(|t| t.done).count();
        if n == 0 {
            self.info("nothing to clear");
            return;
        }
        let id = self.selected_id();
        self.checkpoint();
        self.tasks.retain(|t| !t.done);
        self.changed();
        self.select_id(id);
        self.info(format!("cleared {n} done · u to undo"));
    }

    pub fn toggle_hide_done(&mut self) {
        let id = self.selected_id();
        self.hide_done = !self.hide_done;
        self.select_id(id);
    }

    pub fn start_move(&mut self) {
        if self.selected().is_some() {
            self.mode = Mode::Move;
            self.move_checkpointed = false;
        }
    }

    /// Move the selected task past its visible neighbour. `down` picks the direction.
    pub fn move_selected(&mut self, down: bool) {
        let visible = self.visible();
        let Some(&from) = visible.get(self.cursor) else {
            return;
        };
        let neighbour = if down {
            self.cursor.checked_add(1)
        } else {
            self.cursor.checked_sub(1)
        };
        let Some(&to) = neighbour.and_then(|n| visible.get(n)) else {
            return;
        };

        // One undo step per move-mode session, one per J/K otherwise.
        if self.mode != Mode::Move || !self.move_checkpointed {
            self.checkpoint();
            self.move_checkpointed = self.mode == Mode::Move;
        }
        let task = self.tasks.remove(from);
        self.tasks.insert(to, task);
        self.cursor = if down { self.cursor + 1 } else { self.cursor - 1 };
        self.changed();
    }

    pub fn done_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.done).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn app(names: &[&str]) -> App {
        let mut app = App::new(Vec::new());
        app.today = today;
        for n in names {
            add(&mut app, n);
        }
        app.dirty = false;
        app.cursor = 0;
        app
    }

    fn add(app: &mut App, text: &str) {
        app.start_insert(Target::Append);
        app.editor.insert_str(text);
        app.submit_input();
    }

    fn names(app: &App) -> Vec<&str> {
        app.tasks.iter().map(|t| t.description.as_str()).collect()
    }

    fn selected_name(app: &App) -> Option<&str> {
        app.selected().map(|i| app.tasks[i].description.as_str())
    }

    #[test]
    fn empty_list_is_safe() {
        let mut a = app(&[]);
        a.next();
        a.previous();
        a.last();
        a.first();
        a.page_down();
        a.page_up();
        a.toggle();
        a.delete();
        a.cycle_priority();
        a.move_selected(true);
        a.move_selected(false);
        a.start_edit();
        a.start_move();
        assert_eq!(a.mode, Mode::Normal);
        assert_eq!(a.cursor, 0);
        assert_eq!(a.selected(), None);
        assert!(!a.dirty);
    }

    #[test]
    fn single_item() {
        let mut a = app(&["one"]);
        a.next();
        assert_eq!(a.cursor, 0);
        a.move_selected(true);
        a.move_selected(false);
        assert_eq!(names(&a), ["one"]);
        a.toggle();
        assert!(a.tasks[0].done);
        assert!(a.tasks[0].completed.is_some());
        a.toggle();
        assert!(!a.tasks[0].done);
        assert!(a.tasks[0].completed.is_none());
        a.delete();
        assert!(a.tasks.is_empty());
        assert_eq!(a.selected(), None);
    }

    #[test]
    fn navigation_bounds() {
        let mut a = app(&["a", "b", "c"]);
        a.previous();
        assert_eq!(a.cursor, 0);
        a.last();
        assert_eq!(a.cursor, 2);
        a.next();
        assert_eq!(a.cursor, 2);
        a.page_size = 10;
        a.page_up();
        assert_eq!(a.cursor, 0);
        a.page_down();
        assert_eq!(a.cursor, 2);
    }

    #[test]
    fn delete_last_moves_cursor_up() {
        let mut a = app(&["a", "b", "c"]);
        a.last();
        a.delete();
        assert_eq!(names(&a), ["a", "b"]);
        assert_eq!(selected_name(&a), Some("b"));
        assert!(a.dirty);
        assert!(a.status.as_ref().unwrap().text.contains("undo"));
    }

    #[test]
    fn insert_positions() {
        let mut a = app(&["a", "c"]);
        a.start_insert(Target::Below);
        a.editor.insert_str("b");
        a.submit_input();
        assert_eq!(names(&a), ["a", "b", "c"]);
        assert_eq!(selected_name(&a), Some("b"));
        a.start_insert(Target::Above);
        a.editor.insert_str("x");
        a.submit_input();
        assert_eq!(names(&a), ["a", "x", "b", "c"]);
        assert_eq!(selected_name(&a), Some("x"));
        // Ids stay unique.
        let mut ids: Vec<_> = a.tasks.iter().map(|t| t.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn empty_input_is_ignored() {
        let mut a = app(&["a"]);
        add(&mut a, "   ");
        add(&mut a, "!!");
        assert_eq!(names(&a), ["a"]);
        assert!(!a.dirty);
    }

    #[test]
    fn inline_syntax_on_add_and_edit() {
        let mut a = app(&["buy milk !! @tom"]);
        let t = &a.tasks[0];
        assert_eq!(t.description, "buy milk");
        assert_eq!(t.priority, 2);
        assert_eq!(t.due, NaiveDate::from_ymd_opt(2026, 10, 23));

        a.start_edit();
        assert_eq!(a.editor.text(), "buy milk !! @2026-10-23");
        for _ in 0..3 {
            a.editor.delete_word_back();
        }
        a.editor.insert_str("oat milk !");
        a.submit_input();
        let t = &a.tasks[0];
        assert_eq!(t.description, "buy oat milk");
        assert_eq!(t.priority, 1);
        assert_eq!(t.due, None);
        a.undo();
        assert_eq!(a.tasks[0].description, "buy milk");
    }

    #[test]
    fn move_respects_bounds() {
        let mut a = app(&["a", "b", "c"]);
        a.move_selected(false);
        assert_eq!(names(&a), ["a", "b", "c"]);
        a.move_selected(true);
        a.move_selected(true);
        a.move_selected(true);
        assert_eq!(names(&a), ["b", "c", "a"]);
        assert_eq!(selected_name(&a), Some("a"));
        a.move_selected(false);
        assert_eq!(names(&a), ["b", "a", "c"]);
        a.undo();
        assert_eq!(names(&a), ["b", "c", "a"]);
    }

    #[test]
    fn move_mode_is_one_undo_step() {
        let mut a = app(&["a", "b", "c"]);
        a.start_move();
        a.move_selected(true);
        a.move_selected(true);
        a.mode = Mode::Normal;
        assert_eq!(names(&a), ["b", "c", "a"]);
        a.undo();
        assert_eq!(names(&a), ["a", "b", "c"]);
    }

    #[test]
    fn undo_redo() {
        let mut a = app(&["a", "b"]);
        a.delete();
        a.toggle();
        assert_eq!(names(&a), ["b"]);
        assert!(a.tasks[0].done);
        a.undo();
        a.undo();
        assert_eq!(names(&a), ["a", "b"]);
        a.redo();
        assert_eq!(names(&a), ["b"]);
        a.redo();
        assert!(a.tasks[0].done);
        a.redo();
        assert_eq!(a.status.as_ref().unwrap().text, "nothing to redo");
        // A new change drops the redo stack.
        a.undo();
        a.cycle_priority();
        a.redo();
        assert!(!a.tasks[0].done);
    }

    #[test]
    fn undo_is_bounded() {
        let mut a = app(&["a"]);
        for _ in 0..150 {
            a.cycle_priority();
        }
        let mut n = 0;
        while !a.undo.is_empty() {
            a.undo();
            n += 1;
        }
        assert_eq!(n, UNDO_LIMIT);
    }

    #[test]
    fn priority_cycles() {
        let mut a = app(&["a"]);
        let mut seen = Vec::new();
        for _ in 0..5 {
            a.cycle_priority();
            seen.push(a.tasks[0].priority);
        }
        assert_eq!(seen, [1, 2, 3, 0, 1]);
    }

    #[test]
    fn filter_with_hidden_done() {
        let mut a = app(&["Buy milk", "call mom", "buy bread", "walk"]);
        a.cursor = 2;
        a.toggle(); // buy bread done
        a.hide_done = true;
        a.clamp_cursor();

        a.start_filter();
        a.editor.insert_str("BUY");
        a.update_filter();
        assert_eq!(a.visible(), [0]);
        assert_eq!(selected_name(&a), Some("Buy milk"));
        a.commit_filter();
        assert_eq!(a.filter, "BUY");

        a.toggle_hide_done();
        assert_eq!(a.visible(), [0, 2]);
        a.last();
        assert_eq!(selected_name(&a), Some("buy bread"));
        // Moving swaps visible neighbours only.
        a.move_selected(false);
        assert_eq!(names(&a), ["buy bread", "Buy milk", "call mom", "walk"]);
        a.toggle_hide_done();
        assert_eq!(a.visible(), [1]);
        assert_eq!(selected_name(&a), Some("Buy milk"));

        a.clear_filter();
        assert_eq!(a.visible(), [1, 2, 3]);
        assert_eq!(selected_name(&a), Some("Buy milk"));
    }

    #[test]
    fn toggling_hidden_keeps_cursor_in_range() {
        let mut a = app(&["a", "b"]);
        a.hide_done = true;
        a.last();
        a.toggle();
        assert_eq!(a.visible(), [0]);
        assert_eq!(a.cursor, 0);
        a.toggle();
        assert_eq!(a.visible(), Vec::<usize>::new());
        assert_eq!(a.selected(), None);
    }

    #[test]
    fn clear_done_and_undo() {
        let mut a = app(&["a", "b", "c"]);
        a.toggle();
        a.last();
        a.toggle();
        a.clear_done();
        assert_eq!(names(&a), ["b"]);
        a.undo();
        assert_eq!(names(&a), ["a", "b", "c"]);
        a.redo();
        a.clear_done();
        assert_eq!(a.status.as_ref().unwrap().text, "nothing to clear");
    }

    #[test]
    fn reload_drops_history_and_stale_edit() {
        let mut a = app(&["a", "b"]);
        a.delete();
        a.start_edit();
        let remaining = a.tasks.clone();
        a.reload(Vec::new());
        assert_eq!(a.mode, Mode::Normal);
        a.undo();
        assert!(a.tasks.is_empty());
        a.reload(remaining);
        assert_eq!(names(&a), ["b"]);
    }

    #[test]
    fn status_expires() {
        let mut a = app(&[]);
        a.info("hi");
        a.tick();
        assert!(a.status.is_some());
        a.status.as_mut().unwrap().since -= STATUS_TTL;
        a.tick();
        assert!(a.status.is_none());
    }
}
