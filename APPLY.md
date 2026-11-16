# Invertonator-rs — Full Source Package (v0.3.0-alpha.22, holes everywhere)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.22 adds (your "make the holes work per tab, per window, on video and things")

The hole mechanism is proven and both hotkeys work. alpha.22 widens
where holes exist and how you manage them — the Windows v2.1 analogs,
Wayland edition:

1. **Per window — hole manager.** The panel menu gains a `Holes: N`
   submenu listing every hole by window title and app class; click a
   row to close exactly that hole. Pierce a browser, a terminal and a
   PDF viewer at once — three independent holes, each following its
   own window.
2. **Video — fullscreen auto-hole.** While inversion is ON, any window
   that goes fullscreen is pierced AUTOMATICALLY true color: YouTube
   fullscreen, mpv, VLC, games. No hotkey needed — press the browser
   fullscreen key and the hole pops by itself. Un-fullscreening
   removes the auto-hole again. Manual Super+F7 holes are NEVER
   auto-removed. Opt out if you want fullscreen windows to stay
   inverted:
   ```
   GSETTINGS_SCHEMA_DIR="$HOME/.local/share/gnome-shell/extensions/invertonator@phoenix1504e/schemas" \
     gsettings set org.gnome.shell.extensions.invertonator auto-hole-fullscreen false
   ```
3. **Per tab — free, and explained.** A browser window is ONE buffer to
   the compositor; tabs are web content inside it. A hole covers
   whichever tab is showing and follows tab switches with zero work —
   switch to any tab and it stays true color. (The shell cannot tell
   tabs apart; hole-only-the-video-element needs AT-SPI2 rects — v2.2.)
4. **Robustness — holes survive things.** If Mutter recreates a window
   actor (remap paths), the hole reattaches to the new actor; window
   close drops its hole cleanly (no leaked bookkeeping). While
   inverted, direct scanout / unredirect is disabled where the Mutter
   API exists, so a fullscreen video can never bypass the compositor
   shaders. Override-redirect windows (context menus, tooltips) are
   now pierceable too.

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.22`**. If it
prints alpha.21 or older, re-download the zip from the chat first.
(Also: only use the block from the LATEST chat message — an old
scrollback apply block just stops harmlessly at "nothing to commit",
but don't rely on that.)

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.22: Linux v2.1 — holes everywhere (hole manager, fullscreen video auto-hole, actor survival, scanout guard)" && \
git push origin HEAD && \
bash shell-extension/install.sh
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in**. Reinstalling over a copy the running
shell already executes cannot hot-reload — the logout guarantees the
new module actually runs.

After login: install.sh output should include FOUR
`schema check: '...' present` lines (toggle-invert, toggle-pierce,
auto-hole-fullscreen, start-enabled); the journal (command below) must
show both keybindings REGISTERED with their accels.

## Test protocol (5 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 22**, State: ACTIVE.
2. `journalctl --user -b --no-pager | grep -i Invertonator | tail -10` →
   must contain BOTH registered lines:
   - `keybinding 'toggle-invert' registered (action=…, accel=["<Super>i"])`
   - `keybinding 'toggle-pierce' registered (action=…, accel=["<Super>F7"])`
3. Super+I → inverted. Hover Firefox → **Super+F7** → true color. Switch
   tabs a few times → hole stays on every tab (per-tab check).
4. Pierce a SECOND window (terminal) with Super+F7 → panel menu now shows
   `Holes: 2`; open it — one row per hole; click the terminal row → only
   that hole closes (per-window manager check).
5. **Video check**: open a YouTube video → fullscreen it while inverted →
   notification "Auto-hole ON: ..." and the player comes up true color
   with NO hotkey pressed. Esc out of fullscreen → auto-hole removes
   itself. (mpv: same via its fullscreen key.)
6. Manual-hole persistence: pierce a window manually, fullscreen a
   DIFFERENT window (auto-hole), then un-fullscreen the auto one → the
   manual hole survives.
7. Panel indicator shows `Inversion: ON · N holes`; Super+I off → all
   holes clear; Super+I on again → any fullscreen window re-auto-holes.

Report back ONE line: version + registered lines + auto-hole result +
manager result. If anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -30
```

---

## Windows machine (if you still boot it)

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.22: Linux v2.1 — holes everywhere (hole manager, fullscreen video auto-hole, actor survival, scanout guard)"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.22 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (24 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.22 (windows+linux deps)
Cargo.lock                   pinned
Invertonator-rs-v0.2.0-alpha.2.exe  legacy artifact (safe to git rm)
LICENSE                      MIT
README.md                    status + platform matrix (5 platforms) + roadmap
docs/LINUX-TESTING.md        Fedora-first runbook (Wayland ext / X11 / KDE)
icon.ico / icon.svg          logo (tray + README)
src/main.rs                  cross-platform entry (cfg dispatcher)
src/common.rs                settings.json + logging + VERSION
src/win_main.rs              Windows backend (tray, hotkeys, probe, DPI,
                             auto-diagnostics, hysteresis telemetry)
src/holes.rs                 hole pool + main-thread pump (≈64Hz, context-
                             aware evidence, pump heartbeat)
src/tracker.rs               UIA media discovery (cached+live walks, same-
                             name merge, wake loop, fail-open offscreen,
                             sticky trust, ANTI-FLAP HYSTERESIS)
src/scrollhook.rs            low-level wheel hook (scroll-hold, page-dirty)
src/linux.rs                 X11 XRandR gamma inversion + --self-test +
                             Wayland session awareness
shell-extension/install.sh                       installer + schema verification + gsettings fallback
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v22, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           Super+F7 pierce holes, hole
                                                           manager submenu, fullscreen
                                                           auto-hole, actor-recreate
                                                           survival, scanout guard
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
