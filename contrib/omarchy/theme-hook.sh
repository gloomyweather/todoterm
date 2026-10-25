#!/usr/bin/env bash
# Omarchy theme-set hook: recolor the todoterm icon to the theme's foreground,
# so it stays visible on dark and light themes alike.
# install.sh puts it in ~/.config/omarchy/hooks/theme-set.d/todoterm.

ICON="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/todoterm.svg"
COLORS="$HOME/.local/state/omarchy/current/theme/colors.toml"

[[ -f "$ICON" && -f "$COLORS" ]] || exit 0

color="$(sed -nE 's/^foreground[[:space:]]*=[[:space:]]*"(#[0-9a-fA-F]{6})".*/\1/p' "$COLORS" | head -n1)"
[[ -n "$color" ]] || exit 0

sed -i -E "s/stroke=\"#[0-9a-fA-F]{3,8}\"/stroke=\"$color\"/" "$ICON"
# Bump the mtime of the theme dir so icon caches notice the change.
touch "$(dirname "$ICON")/../.."
gtk-update-icon-cache -q -t -f "$(dirname "$ICON")/../.." >/dev/null 2>&1 || true
