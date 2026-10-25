# changelog

## 0.2.0

### fixed
- a broken `todo.json` no longer turns into an empty list that the next save writes over. todoterm refuses to start, keeps a `todo.json.corrupt-<time>` copy and leaves the file alone
- saving is atomic (temp file, fsync, rename), so a crash mid-save can't corrupt the list
- save errors are shown in red instead of being silently ignored; if saving keeps failing, your tasks are written to a temp file on quit
- the terminal is restored after a panic
- `ctrl+c` while typing no longer inserts a `c`; ctrl/alt chords aren't typed at all
- mouse capture is gone, so selecting text in the terminal works again
- readable on light themes: no more hard-coded white and black
- the list no longer writes `todo.json` into whatever folder you started it from (see "where your tasks live")

### added
- tasks live in `~/.local/share/todoterm/todo.json` by default; `./todo.json`, `--file`, `TODOTERM_FILE` and the config can point elsewhere
- outside changes to the file are picked up within a second
- edit (`e`), insert below / above (`o` / `O`), move with `J` / `K`, priorities (`p`), due dates
- inline syntax: `buy milk !! @fri`
- live filter (`/`), hide finished (`h`), clear finished (`C`), undo / redo (`u` / `ctrl+r`), help (`?`)
- a proper line editor: cursor movement, `ctrl+w`, `ctrl+u`, paste
- colors follow the terminal, or the current omarchy theme with live reload on theme switch
- optional `~/.config/todoterm/config.toml`
- omarchy installer with launcher entry, icon, floating window and super + alt + t; a PKGBUILD; release binaries
- `--help`, `--version`

### changed
- built on ratatui 0.29; release builds use lto and are stripped

## 0.1.0
- first release
