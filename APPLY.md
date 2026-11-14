# Invertonator-rs — Full Source Package (v0.3.0-alpha.20, Linux v2 holes)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.20 adds (your "no holes tho")

Linux v2 true-color holes are IN. Global inversion (alpha.19) is
untouched; on top of it:

- **F7 pierce** — hover any window (video player, image viewer,
  whatever) and press **F7**: that window pops to TRUE COLOR while
  everything around it stays inverted. Press F7 over the same window
  again → hole closed. A notification confirms each pierce with the
  window title ("Hole ON: mpv" / "Hole OFF: ...").
- Mechanism: the hole is the SAME inversion shader attached a SECOND
  time, to that window's actor. Inversion is an involution (1 − x), so
  the window is inverted twice = original colors. The hole IS the
  window — dragging, resizing, fullscreening all follow automatically.
  No rectangle tracking, no overlay windows, nothing for a page to see.
- Panel menu got two items: "Pierce window under pointer (F7)" and
  "Clear all holes"; the indicator shows `Inversion: ON · N holes`.
- Turning inversion OFF clears every hole automatically (a lone hole
  would be the only inverted thing on screen) — re-pierce after
  toggling back on.
- F7 grabbed by your keyboard's media row? Rebind:
  `gsettings set org.gnome.shell.extensions.invertonator toggle-pierce "['<Super>h']"`

Known v2 scope: holes are MANUAL (your pick). The Windows UIA
auto-media-hole analog (AT-SPI2 discovery) is v2.1 — trivial once this
round is field-proven.

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.20`**. If it
prints alpha.19 or older, re-download the zip from the chat first.

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.20: Linux v2 — F7 pierce true-color holes (per-window double-invert)" && \
git push origin HEAD && \
bash shell-extension/install.sh
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in**. You are REINSTALLING over a copy the
running shell already executes, and Wayland cannot hot-reload extension
code — the logout guarantees the new module actually runs.

After login: brightness icon in the top-right panel → **Super+I** flips
the whole screen, **F7** pierces a hole into whatever window the pointer
is over.

## Test protocol (3 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 20**, State: ACTIVE.
2. Super+I → whole screen inverts (alpha.19 behavior, unchanged).
3. Hover your video player → **F7** → the player pops to true color,
   everything around it stays inverted; notification "Hole ON: ...".
4. F7 again over the same window → hole closes ("Hole OFF").
5. With a hole open: drag the player around → the hole follows (it IS
   the window). A window stacked ABOVE the hole stays inverted —
   correct: the hole belongs to that window's pixels only.
6. Panel indicator shows `Inversion: ON · 1 hole`; "Clear all holes"
   empties the set.
7. Super+I off → holes clear automatically (indicator back to OFF).
8. F7 while inversion is OFF → notification "Turn inversion on first".

Report back ONE line: state + invert + pierce (which player) + anything
odd (+ `gnome-shell --version`).

If anything is off:

```bash
journalctl --user -b --no-pager | grep -iE 'invertonator|JS ERROR' | tail -30
```

---

## Windows machine (if you still boot it)

```powershell
cd C:\Users\admin\invertonator-push
Expand-Archive -Path "$env:USERPROFILE\Downloads\Invertonator-rs-full-source.zip" -DestinationPath . -Force
git add -A
git commit -m "alpha.20: Linux v2 — F7 pierce true-color holes (per-window double-invert)"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.20 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (24 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.20 (windows+linux deps)
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
shell-extension/install.sh                       installer + gsettings enable fallback
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v20, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           F7 pierce holes, panel indicator
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
