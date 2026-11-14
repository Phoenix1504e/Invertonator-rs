# Invertonator-rs — Full Source Package (v0.3.0-alpha.18, shader-source fix)

Complete repo at a fixed state — extract over your clone, commit, push.
No `.git` inside, your history is untouched.

---

## What alpha.18 fixes (your latest report)

> Journal: `Invertonator: shader effect could not be created
> (TypeError: effect.set_shader_source is not a function)`

alpha.17 proved the import fix works — module loads, enable() runs,
indicator + Super+I all live (the journal showed every one of your
keypresses). But GNOME 51's GJS bindings for `Clutter.ShaderEffect`
do NOT expose `set_shader_source()` — so the effect object never got
its GLSL, construction threw, and every toggle logged
"no shader effect — cannot invert". That is why the screen refused
to flip while everything else worked.

The fix: the GLSL source is a GObject property — `shader-source`,
stable since Clutter 1.10 — so alpha.18 sets it with the raw,
always-available `set_property()` and reads it back; if the property
ever disappears again, it fails loudly in the journal instead of
silently.

Also shipped here:

- install.sh honesty lines: a live enable re-imports on an
  ERROR→enabled transition, but a REINSTALL over a loaded copy keeps
  running the previous module until restart — the script now says
  both instead of overpromising.
- this file grew a zip-freshness gate (step 0 below) after the
  `<path-to-zip>` placeholder incident: the filename is identical
  across versions, so an old download would silently apply nothing.

alpha.17 kept: canonical `import * as Main` ESM imports; install.sh
whitelist fallback for pre-login installs.

Windows side: untouched (alpha.15 code, version bump only).

---

## Step 0 — zip freshness gate

The filename is the SAME across versions. An older download in
`~/Downloads` would unzip nothing new. Check before applying:

```bash
unzip -z ~/Downloads/Invertonator-rs-full-source.zip
```

It must print **`Invertonator-rs <sha> 0.3.0-alpha.18`**. If it
prints alpha.17 or older, re-download the zip from the chat first.

## Apply on Fedora — paste this whole block

```bash
cd ~/FromWindows/Projects/Invertonator-rs && \
git fetch origin && \
git reset --hard origin/main && \
unzip -o ~/Downloads/Invertonator-rs-full-source.zip -d . && \
git status --short && \
git add -A && \
git commit -m "alpha.18: shader source via GObject property (GNOME 51 has no set_shader_source) + installer honesty" && \
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
code — the logout guarantees the new module actually runs. (On a
brand-new install with the shell still in ERROR state, a live enable
does re-import — but never gamble; the logout is 20 seconds.)

After login: brightness icon in the top-right panel → **Super+I** flips
the whole screen.

## Test protocol (2 minutes)

1. `gnome-extensions info invertonator@phoenix1504e` → **Version: 18**, State: ACTIVE.
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
git commit -m "alpha.18: shader source via GObject property (GNOME 51 has no set_shader_source) + installer honesty"
git push origin HEAD
```

Verify: first line of `%LOCALAPPDATA%\Invertonator\log.txt` must read
`=== Invertonator-rs v0.3.0-alpha.18 ===`. Windows behavior is
alpha.15-identical; its test protocol and Desktop diagnostics still apply.

---

## File map (23 files, latest state)

```
.github/workflows/build.yml  CI: windows+ubuntu artifacts + GNOME extension bundle
.gitignore                   /target, *.exe — keep, protects your clone
APPLY.md                     this file
Cargo.toml                   0.3.0-alpha.18 (windows+linux deps)
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
shell-extension/invertonator@phoenix1504e/metadata.json    uuid, v18, shells 45–51
shell-extension/invertonator@phoenix1504e/extension.js     compositor shader
                                                           inversion (GLSL set via
                                                           shader-source property),
                                                           Super+I, panel indicator
shell-extension/invertonator@phoenix1504e/schemas/org.gnome.shell.extensions.invertonator.gschema.xml
shell-extension/invertonator@phoenix1504e/schemas/gschemas.compiled
```
