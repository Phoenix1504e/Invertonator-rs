# Invertonator-rs — Full Source Package (v0.3.0-alpha.21, browser-safe pierce hotkey)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.21 fixes (your "f7 honestly just goes directly to the browser")

The hole mechanism is proven (menu pierce works, snaps to the exact
window). The hotkey wasn't — and here's the verdict from the evidence:

- Super+I being consumed by the compositor proves the whole keybinding
  pipeline works on GNOME 51. If `toggle-pierce` had registered, F7
  would NEVER have reached Firefox. It reached Firefox ⇒ registration
  of an **unmodified F-key grab** is the failure — and plain F7 is also
  browser-claimed (caret browsing in Firefox AND Chromium), so even a
  successful grab would live dangerously.

alpha.21 changes three things, each covering one failure branch:

1. **Default pierce hotkey → `Super+F7`** (browser-safe, no GNOME or
   app claims it; modifier combos are the proven grab path — Super+I
   works). Muscle memory barely changes: Windows+F7.
2. **Registration journals itself** — at every enable you now get
   `keybinding 'toggle-pierce' registered (action=…, accel=…)` in the
   journal, so a silent miss can never again burn a whole field round.
3. **install.sh verifies the installed schema** — after compiling, it
   checks every hotkey key is actually IN the compiled schema and
   screams if not (a stale schema is the classic silent-killer).

Panel menu pierce keeps working exactly as before. Prefer plain F7
anyway? Rebind and test — it will keep reaching Firefox unless the
compositor grabs it:

```
gsettings set org.gnome.shell.extensions.invertonator toggle-pierce "['F7']"
```

(For the gsettings CLI to see extension schemas, prefix with
`GSETTINGS_SCHEMA_DIR="$HOME/.local/share/gnome-shell/extensions/invertonator@phoenix1504e/schemas"`.)

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.21`**. If it
prints alpha.20 or older, re-download the zip from the chat first.
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
git commit -m "alpha.21: pierce hotkey -> Super+F7 (plain F7 falls through to browsers) + registration logging" && \
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

After login: install.sh output should include three
`schema check: '...' present` lines; the journal (command below) must
show both keybindings REGISTERED with their accels.

## Test protocol (3 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 21**, State: ACTIVE.
2. `journalctl --user -b --no-pager | grep -i Invertonator | tail -8` →
   must contain BOTH lines:
   - `keybinding 'toggle-invert' registered (action=…, accel=["<Super>i"])`
   - `keybinding 'toggle-pierce' registered (action=…, accel=["<Super>F7"])`
3. Super+I → whole screen inverts (unchanged).
4. Hover your video player → **Super+F7** → hole pops true color,
   notification "Hole ON: ...". Super+F7 again over it → hole closes.
5. Panel menu pierce still works; indicator shows the hole count.
6. Super+I off → holes clear.

Report back ONE line: state + registered lines + Super+F7 pierce result.
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
git commit -m "alpha.21: pierce hotkey -> Super+F7 (plain F7 falls through to browsers) + registration logging"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.21 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (24 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.21 (windows+linux deps)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v21, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader inversion
                                                           (Cogl.Snippet / dual-API effect,
                                                           uiGroup attach), Super+I,
                                                           Super+F7 pierce holes, panel
                                                           indicator, registration logging
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
worklog.md                   project memory: tasks, field findings, SHA chain
```
