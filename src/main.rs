mod app;
mod config;
mod editor;
mod keys;
mod storage;
mod task;
mod theme;
mod ui;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind},
    execute,
    terminal::SetTitle,
};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::config::Config;
use crate::storage::Storage;
use crate::theme::ThemeSource;

/// How often to look for outside changes to the task file and the theme.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);
const POLL_TIMEOUT: Duration = Duration::from_millis(250);

/// A minimalist and aesthetic terminal task manager.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Task file to use [default: ./todo.json if present, else ~/.local/share/todoterm/todo.json]
    #[arg(short, long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Config file [default: ~/.config/todoterm/config.toml]
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("todoterm: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.or_else(Config::default_path).as_deref())?;
    let path = storage::resolve_path(
        cli.file,
        std::env::var_os("TODOTERM_FILE"),
        config.file.clone(),
        &std::env::current_dir().context("could not get the current directory")?,
        dirs::data_dir(),
    )?;

    // Everything that can fail on startup happens before the terminal is touched,
    // so errors print normally.
    let mut storage = Storage::new(path);
    let tasks = storage.load()?;
    let mut theme = ThemeSource::new(config.theme, dirs::home_dir().as_deref(), config.colors.clone())?;

    let mut app = App::new(tasks);
    app.kaomoji = config.kaomoji;
    app.hide_done = config.hide_done;

    let mut terminal = ratatui::init();
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(stdout(), DisableBracketedPaste);
        hook(info);
    }));
    let _ = execute!(stdout(), EnableBracketedPaste, SetTitle("todoterm"));

    let result = event_loop(&mut terminal, &mut app, &mut storage, &mut theme);

    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    result?;
    final_save(&mut app, &mut storage)
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    storage: &mut Storage,
    theme: &mut ThemeSource,
) -> io::Result<()> {
    let mut last_watch = Instant::now();
    loop {
        let palette = theme.palette();
        terminal.draw(|f| ui::draw(f, app, &palette))?;

        if event::poll(POLL_TIMEOUT)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => keys::handle_key(app, key),
                Event::Paste(text) => keys::handle_paste(app, &text),
                _ => {}
            }
        }

        if app.dirty {
            save(app, storage);
        }
        if app.should_quit {
            return Ok(());
        }

        if last_watch.elapsed() >= WATCH_INTERVAL {
            last_watch = Instant::now();
            if !app.unsaved && storage.changed_externally() {
                match storage.load() {
                    Ok(tasks) => app.reload(tasks),
                    Err(e) => app.error(format!("{e:#}")),
                }
            }
            theme.poll();
        }
        app.tick();
    }
}

fn save(app: &mut App, storage: &mut Storage) {
    app.dirty = false;
    match storage.save(&app.tasks) {
        Ok(()) => app.unsaved = false,
        Err(e) => {
            app.unsaved = true;
            app.error(format!("{e:#}"));
        }
    }
}

/// If saving kept failing, try once more and otherwise park the tasks somewhere safe.
fn final_save(app: &mut App, storage: &mut Storage) -> Result<()> {
    if !app.unsaved {
        return Ok(());
    }
    let Err(err) = storage.save(&app.tasks) else {
        return Ok(());
    };
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let rescue = std::env::temp_dir().join(format!("todoterm-unsaved-{ts}.json"));
    let json = serde_json::to_string_pretty(&app.tasks)?;
    std::fs::write(&rescue, json)
        .with_context(|| format!("{err:#}; also could not write {}", rescue.display()))?;
    anyhow::bail!(
        "{err:#}\nyour tasks were written to {} instead (target: {})",
        rescue.display(),
        storage.path().display()
    )
}
