#!/usr/bin/env bash
# Invertonator — GNOME Shell extension installer (Wayland sessions).
#
# Installs the extension into your user extension directory, recompiles
# the settings schema (falls back to the shipped gschemas.compiled when
# the compiler is absent), and enables it. First activation needs one
# logout/login on Wayland; afterwards Super+I toggles inversion.
set -euo pipefail

UUID="invertonator@phoenix1504e"
SRC="$(cd "$(dirname "$0")/$UUID" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"

if [ "$(id -u)" = "0" ]; then
    echo "error: run as your own user, not root (extensions are per-user)." >&2
    exit 1
fi

echo "installing $UUID"
echo "  $SRC -> $DEST"
mkdir -p "$(dirname "$DEST")"
rm -rf "$DEST"
cp -r "$SRC" "$DEST"

if command -v glib-compile-schemas >/dev/null 2>&1; then
    glib-compile-schemas "$DEST/schemas/"
else
    echo "note: glib-compile-schemas not found — using the shipped precompiled schema"
fi

gnome-extensions enable "$UUID" 2>/dev/null || true

echo
echo "done. TWO cases:"
echo "  * first install on Wayland: log out and log back in once —"
echo "    the extension loads at the next login."
echo "  * X11 session: Alt+F2, type r, Enter (shell restart), or just log out/in."
echo
echo "then press Super+I (or use the new panel indicator) to invert."
echo
echo "uninstall: gnome-extensions disable $UUID && rm -rf '$DEST'"
