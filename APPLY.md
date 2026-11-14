# Invertonator-rs — Full Source Package (v0.3.0-alpha.17, extension hotfix)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.17 fixes (your latest report)

> State: ERROR, "nothing happened", journal:
> `SyntaxError: ... 'resource:///org/gnome/shell/ui/main.js' doesn't provide an export named: 'default'`

Root cause (my bug, your journal pointed straight at it): GNOME Shell's
ui modules (`main.js`, `panelMenu.js`, `popupMenu.js`) export **named
bindings only** — they have NO default export. alpha.16 imported them as
`import Main from ...`, which dies at module load before `enable()` ever
runs → State ERROR → no panel icon → Super+I dead. The canonical ESM
template form is `import * as Main from ...`; all three imports are
fixed, plus the ShaderEffect constructor property moved to the canonical
camelCase `shaderType`.

Also fixed while here (second field finding): install.sh's
`gnome-extensions enable` silently failed during a pre-login install
(the running shell has not scanned the extension dir yet), leaving the
extension loaded-but-DISABLED forever. install.sh now whitelists the
uuid in `org.gnome.shell enabled-extensions` via gsettings when the
live enable fails — fresh installs come up ACTIVE at the next login
with zero manual steps.

Windows side: untouched (alpha.15 code, version bump only).

---

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.17: fix extension ESM imports (main.js default export) + installer enable fallback" && \
git push origin HEAD
```

(`&&` chaining so a failed step stops the rest; if `git status --short`
ever lists files that are NOT repo files, stop and tell me.)

## Activate — one command + one logout

```bash
bash shell-extension/install.sh
```

Then **log out and log back in** (Wayland loads extension code only at
login — even a fixed file cannot hot-reload into the running shell).
Your uuid is already whitelisted in enabled-extensions from the manual
enable earlier, so it comes up ACTIVE immediately.

After login: brightness icon in the top-right panel → **Super+I** flips
the whole screen.

## Test protocol (2 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → State: ACTIVE.
2. Super+I inverts instantly; text stays perfectly crisp.
3. Toggle off/on a few times — no flicker.
4. YouTube plays inverted (expected — global inversion is v1; true-color
   holes are the v2 milestone).
5. Reboot with inversion ON → session comes back inverted.

Report back ONE line: state + invert + crisp + flicker (+ `gnome-shell --version`).

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
git commit -m "alpha.17: fix extension ESM imports (main.js default export) + installer enable fallback"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.17 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (23 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.17 (windows+linux deps)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v17, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader
                                                           inversion (ESM namespace
                                                           imports fixed), Super+I,
                                                           panel indicator
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
```
