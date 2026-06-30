use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{fs, io, path::PathBuf};

/// Represents a single task in the todo list
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Task {
    id: u32,
    description: String,
    done: bool,
}

/// Defines the different modes the application can be in
#[derive(PartialEq)]
enum AppMode {
    Normal,
    Insert,
    Move,
}

/// Holds the entire state of the application
struct App {
    tasks: Vec<Task>,
    state: ListState,
    mode: AppMode,
    input: String,
    file_path: PathBuf,
    should_quit: bool,
}

impl App {
    /// Initializes the application state and loads existing tasks from the disk
    fn new(file_name: &str) -> Result<Self> {
        let mut path = std::env::current_dir().context("could not get current dir")?;
        path.push(file_name);

        // Load existing tasks if the file exists
        let tasks = if path.exists() {
            let data = fs::read_to_string(&path)?;
            if data.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&data).unwrap_or_default()
            }
        } else {
            Vec::new()
        };

        let mut app = Self {
            tasks,
            state: ListState::default(),
            mode: AppMode::Normal,
            input: String::new(),
            file_path: path,
            should_quit: false,
        };
        
        // Select the first task by default if the list is not empty
        if !app.tasks.is_empty() {
            app.state.select(Some(0));
        }
        Ok(app)
    }

    /// Serializes the current tasks to JSON and saves them to the file
    fn save(&self) -> Result<()> {
        let data = serde_json::to_string_pretty(&self.tasks)?;
        fs::write(&self.file_path, data)?;
        Ok(())
    }

    /// Selects the next task in the list, bounded by the list length
    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => if i >= self.tasks.len().saturating_sub(1) { self.tasks.len().saturating_sub(1) } else { i + 1 },
            None => 0,
        };
        self.state.select(Some(i));
    }

    /// Selects the previous task in the list, bounded by 0 (the top of the list)
    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => if i == 0 { 0 } else { i - 1 },
            None => 0,
        };
        self.state.select(Some(i));
    }

    /// Swaps the currently selected task with the one directly above it
    fn move_task_up(&mut self) {
        if let Some(i) = self.state.selected() {
            if i > 0 {
                self.tasks.swap(i, i - 1);
                self.state.select(Some(i - 1));
                let _ = self.save();
            }
        }
    }

    /// Swaps the currently selected task with the one directly below it
    fn move_task_down(&mut self) {
        if let Some(i) = self.state.selected() {
            if i < self.tasks.len() - 1 {
                self.tasks.swap(i, i + 1);
                self.state.select(Some(i + 1));
                let _ = self.save();
            }
        }
    }

    /// Toggles the completion status (done/not done) of the currently selected task
    fn toggle_current(&mut self) {
        if let Some(i) = self.state.selected() {
            if let Some(task) = self.tasks.get_mut(i) {
                task.done = !task.done;
                let _ = self.save();
            }
        }
    }

    /// Removes the currently selected task and adjusts the selection cursor
    fn delete_current(&mut self) {
        if let Some(i) = self.state.selected() {
            self.tasks.remove(i);
            let _ = self.save();
            // Adjust cursor after deletion to avoid out-of-bounds selection
            if self.tasks.is_empty() {
                self.state.select(None);
            } else if i >= self.tasks.len() {
                self.state.select(Some(self.tasks.len() - 1));
            }
        }
    }

    /// Adds a new task based on the current user input buffer
    fn add_task(&mut self) {
        if self.input.trim().is_empty() { return; }
        // Generate a unique ID by finding the maximum ID currently used and adding 1
        let next_id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task { id: next_id, description: self.input.drain(..).collect(), done: false });
        // Select the newly added task automatically
        self.state.select(Some(self.tasks.len() - 1));
        let _ = self.save();
    }
}

fn main() -> Result<()> {
    // Setup the terminal in raw mode to listen to precise key presses natively
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Initialize application state
    let mut app = App::new("todo.json")?;
    
    // Run the main application event loop
    let res = run_app(&mut terminal, &mut app);
    
    // Restore terminal to its original state even if the app panicked/errored
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    
    // Print any errors that occurred during the runtime
    if let Err(err) = res { println!("{:?}", err); }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        // Draw the UI on every frame
        terminal.draw(|f| ui(f, app))?;
        
        // Wait for and handle terminal events
        if let Event::Key(key) = event::read()? {
            // Ignore key release events, only react to key presses
            if key.kind != KeyEventKind::Press { continue; }
            
            match app.mode {
                AppMode::Normal => match key.code {
                    KeyCode::Char('q') => app.should_quit = true,
                    // Navigation controls
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    // Switch to insert mode to add a new task
                    KeyCode::Char('a') | KeyCode::Char('i') => app.mode = AppMode::Insert,
                    // Switch to move mode to reorder tasks
                    KeyCode::Char('m') => app.mode = AppMode::Move,
                    // Delete the selected task
                    KeyCode::Char('d') | KeyCode::Char('x') => app.delete_current(),
                    // Toggle task completion
                    KeyCode::Enter | KeyCode::Char(' ') => app.toggle_current(),
                    _ => {}
                },
                AppMode::Insert => match key.code {
                    // Save the new task and return to normal mode
                    KeyCode::Enter => { app.add_task(); app.mode = AppMode::Normal; }
                    // Type characters into the input buffer
                    KeyCode::Char(c) => app.input.push(c),
                    // Remove the last character from the input buffer
                    KeyCode::Backspace => { let _ = app.input.pop(); }
                    // Cancel insertion and return to normal mode without saving
                    KeyCode::Esc => { app.mode = AppMode::Normal; app.input.clear(); }
                    _ => {}
                },
                AppMode::Move => match key.code {
                    // Move the selected task down
                    KeyCode::Char('j') | KeyCode::Down => app.move_task_down(),
                    // Move the selected task up
                    KeyCode::Char('k') | KeyCode::Up => app.move_task_up(),
                    // Exit move mode and return to normal mode
                    KeyCode::Esc | KeyCode::Enter => app.mode = AppMode::Normal,
                    _ => {}
                }
            }
        }
        // Check if we should break the main loop and exit
        if app.should_quit { return Ok(()); }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    // Split the terminal window into 3 vertical sections: Header, List area, and Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(3)].as_ref())
        .split(f.size());

    // 1. Render the header with a cute title depending on the app mode
    let title_text = match app.mode {
        AppMode::Normal => " ~ todo list ~  (=^･ω･^=) ",
        AppMode::Insert => " ~ adding task ~  (✍ ˘ ³˘) ",
        AppMode::Move => " ~ moving task ~  (⬍ ˘ ³˘) ",
    };
    
    let title = Paragraph::new(title_text)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::BOTTOM).border_type(BorderType::Plain).style(Style::default().fg(Color::DarkGray)));
    f.render_widget(title, chunks[0]);

    // 2. Render the task list
    let tasks: Vec<ListItem> = app.tasks.iter().map(|t| {
        // Apply a crossed-out style and change brackets if the task is done
        let (status, style) = if t.done {
            (" [x] ", Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT))
        } else {
            (" [ ] ", Style::default().fg(Color::White))
        };
        ListItem::new(Line::from(Span::styled(format!("{}{}", status, t.description), style)))
    }).collect();

    // Show a special calming message if the list is completely empty
    if app.tasks.is_empty() && app.mode == AppMode::Normal {
        let empty_msg = Paragraph::new("\n\n   no tasks left. you can rest now ૮ ˶ᵔ ᵕ ᵔ˶ ა")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty_msg, chunks[1]);
    } else {
        let list = List::new(tasks)
            // Highlight the currently selected task, turning it dark grey if we are actively moving it
            .highlight_style(Style::default().bg(if app.mode == AppMode::Move { Color::DarkGray } else { Color::White }).fg(Color::Black).add_modifier(Modifier::BOLD))
            .highlight_symbol(" › ");
        f.render_stateful_widget(list, chunks[1], &mut app.state);
    }

    // 3. Render the footer with keybind instructions or the active input field
    let footer_content = match app.mode {
        AppMode::Normal => Paragraph::new(" j/k: move | a: add | d: delete | m: reorder | enter: toggle | q: quit "),
        AppMode::Insert => Paragraph::new(format!(" > {}_", app.input)),
        AppMode::Move => Paragraph::new(" j/k: swap pos | esc: done "),
    }.style(Style::default().fg(Color::DarkGray)).block(Block::default().borders(Borders::TOP).border_type(BorderType::Plain));
    f.render_widget(footer_content, chunks[2]);
}