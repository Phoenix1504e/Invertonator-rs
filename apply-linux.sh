#!/usr/bin/env bash
# Invertonator — ONE-command Fedora apply (this copy: v0.3.0 stable / Linux v2.4.0).
#
# Why this script exists: the Windows PowerShell apply block has been
# pasted into Fedora bash TWICE now (round 10 and round 14) — bash eats
# the backslashes in `cd C:\Users\...`, has no Expand-Archive, and git
# then runs "add/commit/push" against a repo the zip never touched
# ("nothing to commit, working tree clean" / "Everything up-to-date").
# This script makes the Fedora path ONE short paste and self-checks
# everything the manual flow got wrong by hand:
#   1. refuses to run anywhere but Linux bash;
#   2. verifies the zip IS v0.3.0 BEFORE touching the repo;
#   3. refuses a dirty repo (except its own drop-in copy);
#   4. commits with the correct, release-pinned message;
#   5. pushes and runs the installer, with the logout reminder.
#
# Usage (the one-liner in APPLY.md — "Apply on Fedora"):
#   cd ~/FromWindows/Projects/Invertonator-rs && \
#   unzip -o ~/Downloads/Invertonator-rs-full-source.zip apply-linux.sh -d . && \
#   bash apply-linux.sh
# Optional: `bash apply-linux.sh /path/to/zip` to point at a zip elsewhere.
#
# Per-release: bump REPO_V + MSG below; this script ships INSIDE the zip.
set -euo pipefail

REPO_V=30
MSG="v0.3.0 stable: desktop-empty auto-off — last real window closing turns inversion off (v2.4.0; includes alpha.29 click fix)"
EXT_DIR="shell-extension/invertonator@phoenix1504e"
# Content markers this release MUST contain (freshness + feature gates).
MARK1='Confirm area'          # alpha.28 adjust+confirm flow
MARK2='auto-off-no-windows'  # v2.4 desktop-empty auto-off

err() {
    echo
    echo "apply-linux: ✗ $*" >&2
    echo "apply-linux: aborted — the repo was NOT modified by the failed step." >&2
    exit 1
}

banner() { echo "apply-linux: $*"; }

# --- guard 1: right machine -------------------------------------------------
[ "$(uname -s)" = "Linux" ] \
    || err "wrong machine — this is the FEDORA block. The PowerShell block belongs on Windows."

# --- guard 2: right repo ----------------------------------------------------
git rev-parse --is-inside-work-tree >/dev/null 2>&1 \
    || err "not inside the git clone — cd ~/FromWindows/Projects/Invertonator-rs first"
[ -f "$EXT_DIR/extension.js" ] \
    || err "wrong directory — $EXT_DIR/extension.js missing"

# --- guard 3: find the zip --------------------------------------------------
ZIP="${1:-}"
for c in "$HOME/Downloads/Invertonator-rs-full-source.zip" \
         "$PWD/Invertonator-rs-full-source.zip" \
         "$HOME/FromWindows/Invertonator-rs-full-source.zip"; do
    if [ -z "$ZIP" ] && [ -f "$c" ]; then
        ZIP="$c"
    fi
done
[ -n "$ZIP" ] && [ -f "$ZIP" ] \
    || err "zip not found (tried arg, ~/Downloads, \$PWD, ~/FromWindows) — download the v0.3.0 zip from the chat first"
banner "zip = $ZIP"
banner "zip header: $(unzip -z "$ZIP" 2>/dev/null | tail -1 || echo '(none)')"

# --- guard 4: verify the zip BEFORE touching the repo ------------------------
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
unzip -q "$ZIP" -d "$TMP"
VER="$(grep -oP '"version":\s*\K[0-9]+' "$TMP/$EXT_DIR/metadata.json" 2>/dev/null || echo 0)"
[ "$VER" = "$REPO_V" ] \
    || err "STALE ZIP (metadata version $VER, need $REPO_V) — re-download the zip from the chat. Repo untouched."
grep -q "$MARK1" "$TMP/$EXT_DIR/extension.js" \
    || err "zip content check failed (no adjust+confirm markers) — re-download the zip."
grep -q "$MARK2" "$TMP/$EXT_DIR/extension.js" \
    || err "zip content check failed (no desktop-empty auto-off marker) — re-download the zip."
grep -q "$MARK2" "$TMP/$EXT_DIR/schemas/org.gnome.shell.extensions.invertonator.gschema.xml" \
    || err "zip content check failed (schema missing auto-off-no-windows) — re-download the zip."
banner "zip verified: v0.3.0 (metadata $REPO_V), adjust+confirm + auto-off markers present"

# --- guard 5: clean repo (this script's own drop-in copy is exempt) ----------
git fetch origin
DIRTY="$(git status --porcelain | grep -v 'apply-linux.sh' || true)"
[ -z "$DIRTY" ] \
    || err "repo has local changes — inspect them first, then re-run:
$DIRTY"
git reset --hard origin/main

# --- apply -------------------------------------------------------------------
cp -a "$TMP/." .
rm -rf "$TMP"
trap - EXIT
banner "extracted — changed files:"
git status --short | sed 's/^/    /'
git add -A
if git diff --cached --quiet; then
    banner "zip matches the repo — nothing to commit (re-run/idempotent apply)"
else
    git commit -m "$MSG"
    git push origin HEAD
    banner "committed + pushed: $MSG"
fi

# --- install ------------------------------------------------------------------
if [ "${INVT_SKIP_INSTALL:-0}" = "1" ]; then
    banner "INVT_SKIP_INSTALL=1 — skipped install.sh (script testing)"
else
    bash shell-extension/install.sh
fi
banner "DONE — now LOG OUT AND BACK IN (GNOME cannot hot-reload the extension)."
banner "Then Super+I → Super+F8 → drag → adjust the handles → Confirm (or Enter)."
banner "Stable-release test: invert → close the last window → inversion auto-off + notification."
