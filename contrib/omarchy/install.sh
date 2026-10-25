#!/usr/bin/env bash
# Install todoterm for the current user and hook it into Omarchy:
# binary, icon, launcher entry, floating window rule and a Super+Alt+T hotkey.
# Safe to run again: everything is replaced, nothing is duplicated.

set -euo pipefail

usage() {
  cat <<'EOF'
usage: contrib/omarchy/install.sh [--yes] [--no-build] [--no-hypr]

  --yes       change the Hyprland config without asking
  --no-build  install target/release/todoterm as is, without running cargo
              (not needed when run from a release tarball)
  --no-hypr   do not touch the Hyprland config
EOF
}

YES=0
BUILD=1
HYPR=1
for arg in "$@"; do
  case "$arg" in
    -y | --yes) YES=1 ;;
    --no-build) BUILD=0 ;;
    --no-hypr) HYPR=0 ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
HYPR_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/hypr"
APP_ID="org.omarchy.todoterm"

say() { printf '\033[1m::\033[0m %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# Replace the block between the markers in $1 with $4 (or append it).
# $2/$3 are the begin/end marker lines. An empty $4 just removes the block.
put_block() {
  local file="$1" begin="$2" end="$3" body="$4" tmp
  tmp="$(mktemp)"
  if [[ -f "$file" ]]; then
    awk -v b="$begin" -v e="$end" '
      $0 == b { skip = 1; next }
      $0 == e { skip = 0; next }
      !skip { print }
    ' "$file" >"$tmp"
    # Drop trailing blank lines left behind by a previous block.
    sed -i -e ':a' -e '/^\n*$/{$d;N;ba' -e '}' "$tmp"
  fi
  if [[ -n "$body" ]]; then
    printf '\n%s\n%s\n%s\n' "$begin" "$body" "$end" >>"$tmp"
  fi
  # Write in place so symlinks and permissions survive.
  cat "$tmp" >"$file"
  rm -f "$tmp"
}

confirm() {
  ((YES)) && return 0
  if [[ ! -t 0 ]]; then
    return 1
  fi
  if have gum; then
    gum confirm "$1"
  else
    local reply
    read -r -p "$1 [y/N] " reply
    [[ "$reply" == [yY]* ]]
  fi
}

# 1. Binary. A release tarball ships it prebuilt next to this folder.
BIN_SRC="$REPO/target/release/todoterm"
if [[ ! -f "$REPO/Cargo.toml" && -x "$REPO/todoterm" ]]; then
  BUILD=0
  BIN_SRC="$REPO/todoterm"
fi
if ((BUILD)); then
  say "building release binary"
  cargo build --release --locked --manifest-path "$REPO/Cargo.toml"
fi
if [[ ! -x "$BIN_SRC" ]]; then
  echo "error: $BIN_SRC not found, run without --no-build" >&2
  exit 1
fi
install -Dm755 "$BIN_SRC" "$BIN_DIR/todoterm"
say "installed $BIN_DIR/todoterm"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "note: $BIN_DIR is not in your PATH" ;;
esac

# 2. Icon.
install -Dm644 "$REPO/assets/todoterm.svg" "$DATA_DIR/icons/hicolor/scalable/apps/todoterm.svg"
gtk-update-icon-cache -q -t -f "$DATA_DIR/icons/hicolor" >/dev/null 2>&1 || true

# Keep the icon in the colors of the current Omarchy theme.
OMARCHY_HOOKS="${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/hooks"
if [[ -d "$OMARCHY_HOOKS" ]]; then
  install -Dm755 "$REPO/contrib/omarchy/theme-hook.sh" "$OMARCHY_HOOKS/theme-set.d/todoterm"
  bash "$OMARCHY_HOOKS/theme-set.d/todoterm"
  say "installed theme hook, the icon follows the Omarchy theme"
fi

# 3. Launcher entry (shows up in Walker with Super+Space).
OMARCHY=0
if have omarchy-launch-or-focus-tui; then
  OMARCHY=1
  EXEC="omarchy-launch-or-focus-tui todoterm"
else
  EXEC="xdg-terminal-exec --app-id=$APP_ID -e todoterm"
fi
mkdir -p "$DATA_DIR/applications"
cat >"$DATA_DIR/applications/todoterm.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Todo
GenericName=Task list
Comment=Minimalist terminal task manager
Exec=$EXEC
Icon=todoterm
Terminal=false
Categories=Utility;
Keywords=todo;tasks;terminal;
StartupWMClass=$APP_ID
EOF
update-desktop-database -q "$DATA_DIR/applications" >/dev/null 2>&1 || true
say "installed launcher entry"

# 4. Hyprland: floating window and hotkey.
if ((!HYPR)); then
  exit 0
fi
if ((!OMARCHY)); then
  say "omarchy not found, skipping the Hyprland setup."
  say "to float the window, add a rule for the class $APP_ID to your Hyprland config"
  exit 0
fi

if [[ -f "$HYPR_DIR/hyprland.lua" ]]; then
  MAIN="$HYPR_DIR/hyprland.lua"
  BINDS="$HYPR_DIR/bindings.lua"
  BEGIN="-- >>> todoterm >>>"
  END="-- <<< todoterm <<<"
  RULE='o.window("org\\.omarchy\\.todoterm", { tag = "+floating-window" })'
  BIND='o.bind("SUPER + ALT + T", "Todo", { tui = "todoterm", focus = true })'
elif [[ -f "$HYPR_DIR/hyprland.conf" ]]; then
  MAIN="$HYPR_DIR/hyprland.conf"
  BINDS="$HYPR_DIR/bindings.conf"
  BEGIN="# >>> todoterm >>>"
  END="# <<< todoterm <<<"
  RULE='windowrule = float, class:^(org\.omarchy\.todoterm)$
windowrule = size 875 600, class:^(org\.omarchy\.todoterm)$
windowrule = center, class:^(org\.omarchy\.todoterm)$'
  BIND='bindd = SUPER ALT, T, Todo, exec, omarchy-launch-or-focus-tui todoterm'
else
  say "no Hyprland config found in $HYPR_DIR, skipping"
  exit 0
fi

if ! confirm "add a floating window rule and Super+Alt+T to $HYPR_DIR?"; then
  say "skipped the Hyprland setup (run with --yes to apply it)"
  exit 0
fi

if [[ -f "$BINDS" ]]; then
  put_block "$MAIN" "$BEGIN" "$END" "$RULE"
  put_block "$BINDS" "$BEGIN" "$END" "$BIND"
  say "updated $MAIN and $BINDS"
else
  put_block "$MAIN" "$BEGIN" "$END" "$RULE
$BIND"
  say "updated $MAIN"
fi

if have hyprctl; then
  hyprctl reload >/dev/null 2>&1 || true
fi
say "done. press Super+Alt+T to open todoterm"
