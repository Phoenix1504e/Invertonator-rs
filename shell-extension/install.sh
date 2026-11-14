#!/usr/bin/env bash
# Invertonator — GNOME Shell extension installer (Wayland sessions).
#
# Installs the extension into your user extension directory, recompiles
# the settings schema (falls back to the shipped gschemas.compiled when
# the compiler is absent), and ENABLES it — live when the running shell
# already knows the extension, otherwise by whitelisting the uuid in
# org.gnome.shell enabled-extensions so the NEXT login comes up active.
# (alpha.16 lesson: a pre-login install makes `gnome-extensions enable`
# fail silently — the shell has not scanned the directory yet — and the
# extension then loads DISABLED forever.)
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

if gnome-extensions enable "$UUID" 2>/dev/null; then
    echo "enabled LIVE — the running shell already knew the extension"
    echo "no logout needed; press Super+I now"
    NEEDS_RELOGIN="no"
else
    # The running shell has never scanned this directory (first install,
    # session started before the files existed). Whitelist the uuid so
    # the next login loads it ENABLED — a manual `gnome-extensions enable`
    # after relogin would also work, but this removes the extra step.
    echo "running shell does not know the extension yet (pre-login install)"
    echo "whitelisting in org.gnome.shell enabled-extensions ..."
    python3 - "$UUID" <<'PY'
import ast
import subprocess
import sys

uuid = sys.argv[1]


def read_list(key):
    raw = subprocess.check_output(
        ["gsettings", "get", "org.gnome.shell", key], text=True
    ).strip()
    if raw.startswith("@as"):
        raw = raw[4:].strip()
    try:
        val = ast.literal_eval(raw)
    except (ValueError, SyntaxError):
        val = []
    return val if isinstance(val, list) else []


enabled = read_list("enabled-extensions")
disabled = read_list("disabled-extensions")

if uuid not in enabled:
    enabled.append(uuid)
    subprocess.run(
        ["gsettings", "set", "org.gnome.shell", "enabled-extensions", str(enabled)],
        check=True,
    )
if uuid in disabled:
    disabled.remove(uuid)
    subprocess.run(
        ["gsettings", "set", "org.gnome.shell", "disabled-extensions", str(disabled)],
        check=True,
    )
print("whitelisted:", enabled)
PY
    NEEDS_RELOGIN="yes"
fi

echo
if [ "$NEEDS_RELOGIN" = "yes" ]; then
    echo "LAST STEP: log out and log back in ONCE (Wayland only loads new"
    echo "extensions at login). Afterwards Super+I toggles inversion and a"
    echo "brightness icon appears in the top-right panel."
else
    echo "Press Super+I (or use the panel indicator) to invert."
fi
echo
echo "uninstall: gnome-extensions disable $UUID && rm -rf '$DEST'"
