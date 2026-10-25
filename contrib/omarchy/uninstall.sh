#!/usr/bin/env bash
# Remove what install.sh added. Your tasks (todo.json) are left alone.

set -euo pipefail

BIN_DIR="$HOME/.local/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
HYPR_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/hypr"

say() { printf '\033[1m::\033[0m %s\n' "$*"; }

# Remove the block between the markers $2 and $3 from $1, if present.
drop_block() {
  local file="$1" begin="$2" end="$3" tmp
  [[ -f "$file" ]] && grep -qxF -- "$begin" "$file" || return 0
  tmp="$(mktemp)"
  awk -v b="$begin" -v e="$end" '
    $0 == b { skip = 1; next }
    $0 == e { skip = 0; next }
    !skip { print }
  ' "$file" >"$tmp"
  sed -i -e ':a' -e '/^\n*$/{$d;N;ba' -e '}' "$tmp"
  cat "$tmp" >"$file"
  rm -f "$tmp"
  say "cleaned $file"
}

rm -f "$BIN_DIR/todoterm" \
  "$DATA_DIR/icons/hicolor/scalable/apps/todoterm.svg" \
  "$DATA_DIR/applications/todoterm.desktop" \
  "${XDG_CONFIG_HOME:-$HOME/.config}/omarchy/hooks/theme-set.d/todoterm"
gtk-update-icon-cache -q -t -f "$DATA_DIR/icons/hicolor" >/dev/null 2>&1 || true
update-desktop-database -q "$DATA_DIR/applications" >/dev/null 2>&1 || true
say "removed binary, icon and launcher entry"

for f in hyprland.lua bindings.lua; do
  drop_block "$HYPR_DIR/$f" "-- >>> todoterm >>>" "-- <<< todoterm <<<"
done
for f in hyprland.conf bindings.conf; do
  drop_block "$HYPR_DIR/$f" "# >>> todoterm >>>" "# <<< todoterm <<<"
done
if command -v hyprctl >/dev/null 2>&1; then
  hyprctl reload >/dev/null 2>&1 || true
fi

say "your tasks are still in ${DATA_DIR}/todoterm/todo.json (if you used the default path)"
