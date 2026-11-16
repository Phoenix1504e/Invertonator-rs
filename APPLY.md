# Invertonator-rs — Full Source Package (v0.3.0-alpha.23, media lock-on)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.23 adds (your "auto hole that only locks onto images and videos, rest inverted")

This is the Windows UIA-tracker analog for Wayland — **element-level
media lock-on** — plus everything from alpha.22 is untouched:

1. **Media lock-on (new, default ON).** While inversion is on, the
   focused window's AT-SPI2 accessibility tree is walked and every
   image / video / animation element ≥48px gets its OWN true-color
   rect hole. Everything else — text, chrome, UI — stays inverted.
   Rects follow scrolling and tab switches; switching focus to a
   window without media clears them (holes never outlive their
   context — same rule as Windows).
2. **How it's done** (so the evidence makes sense): each rect hole is a
   clipped `Clutter.Clone` of the window actor carrying the invert
   effect — an identity overlay of the window, masked to the element
   rect, double-inverted by the global pass into true color. The
   atspi→logical calibration is MEASURED (frame-width ratio), so HiDPI
   or scaling quirks self-correct. Walks are budgeted and chunked so
   the shell never janks; a hung app is blacklisted for 60s.
3. **If media holes don't appear**, the usual cause is dormant app
   trees. Enable toolkit accessibility once:
   ```
   gsettings set org.gnome.desktop.interface toolkit-accessibility true
   ```
   The journal tells you when this hint applies. Opt out of lock-on
   entirely:
   ```
   gsettings set org.gnome.shell.extensions.invertonator auto-hole-media false
   ```
4. alpha.22 kept: hole manager submenu, fullscreen auto-hole,
   actor-recreate survival, scanout guard, Super+F7 pierce.

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.23`**. If it
prints alpha.22 or older, re-download the zip from the chat first.
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
git commit -m "alpha.23: Linux v2.2 — element media lock-on (AT-SPI2 image/video rect holes)" && \
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

After login: install.sh output should include FIVE
`schema check: '...' present` lines (toggle-invert, toggle-pierce,
auto-hole-media, auto-hole-fullscreen, start-enabled).

## Test protocol (6 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 23**, State: ACTIVE.
2. `journalctl --user -b --no-pager | grep -i Invertonator | tail -12` →
   expect: both `registered` keybinding lines, `AT-SPI2 ready — media
   lock-on roles: image,video,animation` (or `unavailable (Atspi
   import: …)` — paste either), and possibly the
   `toolkit-accessibility is OFF` hint.
3. Super+I → inverted. Focus Firefox on a page with a big image and a
   video. Within ~3s: true-color RECTS appear at exactly those
   elements — text and chrome stay inverted. Panel menu shows
   `media lock-on: N rects (auto)`.
4. Scroll the page → rects follow the elements (may lag ~150ms; that
   is the refresh throttle). Switch tab → rects re-lock to the new
   tab's media. Switch focus to a terminal → rects clear (context
   gone), terminal stays fully inverted.
5. Manual pierce still wins: Super+F7 on a window with media holes →
   whole window true color, element rects drop (no double-invert
   conflict). Super+F7 again → element rects return.
6. If NO rects appear on a media-heavy page: paste the journal tail —
   and run the toolkit-accessibility command from §3, then re-test.

Report back ONE line: version + the AT-SPI2 journal line + whether
image/video rects appeared + whether they followed scroll/tab switches.
If anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -40
```

---

## Windows machine (if you still boot it)

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.23: Linux v2.2 — element media lock-on (AT-SPI2 image/video rect holes)"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.23 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (24 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.23 (windows+linux deps)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v23, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           Super+F7 pierce holes, hole
                                                           manager submenu, fullscreen
                                                           auto-hole, AT-SPI2 media
                                                           lock-on (element rect holes),
                                                           scanout guard
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
